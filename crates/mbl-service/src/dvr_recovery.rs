//! Explicit recovery freezes a captured prefix; only known, deleted recordings are cleaned.
use super::Dvr;
use mbl_core::{
    Actor, Error, ItemId, RecordingId, Result, livetv::RecordingRecoveryPlan, time::now_ms,
};
use mbl_db::repo::m9;
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn missing(error: &Error) -> bool {
    matches!(error,Error::Internal(e) if e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind()==ErrorKind::NotFound))
}

impl Dvr {
    async fn recovery_source(&self, row: &m9::RecordingRow) -> Result<(PathBuf, bool)> {
        // Once linked into place, a snapshot is never opened for writing again.
        if row.output_path.as_deref().is_some_and(|p| {
            Path::new(p) != self.root.join(format!("{}.ts", row.id))
                && Path::new(p) != self.root.join(format!("{}.recovered.ts", row.id))
        }) {
            return Err(Error::Validation("Recording path is outside its managed location".into()));
        }
        let archive = self.root.join(format!("{}.recovered.ts", row.id));
        match self.checked_path(RecordingId(row.id), archive.to_str()).await {
            Ok(path) => Ok((path, true)),
            Err(error) if missing(&error) => Ok((
                self.checked_path(RecordingId(row.id), row.output_path.as_deref()).await?,
                false,
            )),
            Err(error) => Err(error),
        }
    }

    pub async fn preview_recovery(&self, row: &m9::RecordingRow) -> Result<RecordingRecoveryPlan> {
        let mut plan = RecordingRecoveryPlan {
            recording_id: RecordingId(row.id),
            title: row.title.clone(),
            size_bytes: 0,
            existing_item_id: row.item_id.map(ItemId),
            can_recover: false,
            reason: None,
        };
        if row.item_id.is_some() {
            return Ok(plan);
        }
        if row.status != "failed" {
            return Err(Error::Conflict("Only failed recordings can be recovered".into()));
        }
        let (path, snapshot) = match self.recovery_source(row).await {
            Ok(source) => source,
            Err(error) if missing(&error) => {
                plan.reason = Some("No saved capture file is available".into());
                return Ok(plan);
            }
            Err(error) => return Err(error),
        };
        plan.size_bytes = tokio::fs::metadata(path).await?.len();
        if plan.size_bytes == 0 {
            plan.reason = Some("The saved capture is empty".into());
            return Ok(plan);
        }
        if !snapshot {
            let storage = self.storage_status().await;
            if storage
                .available_bytes
                .is_none_or(|n| n.saturating_sub(storage.min_free_bytes) < plan.size_bytes)
            {
                plan.reason =
                    Some("Insufficient free space for a recovery snapshot and DVR reserve".into());
                return Ok(plan);
            }
            if storage.max_recording_bytes.is_some_and(|n| plan.size_bytes > n) {
                plan.reason =
                    Some("Saved capture exceeds the configured recording size limit".into());
                return Ok(plan);
            }
        }
        plan.can_recover = true;
        Ok(plan)
    }

    pub async fn recover(
        &self,
        caller: &Actor,
        id: RecordingId,
        policy: &crate::LiveTvService,
    ) -> Result<ItemId> {
        let _guard = self.gate.lock().await;
        let row = policy.authorize_recording_recovery(caller, id).await?;
        if let Some(item) = row.item_id {
            return Ok(ItemId(item));
        }
        let plan = self.preview_recovery(&row).await?;
        if !plan.can_recover {
            return Err(Error::Unavailable(
                plan.reason.unwrap_or_else(|| "Recovery is unavailable".into()),
            ));
        }
        let (source, snapshot) = self.recovery_source(&row).await?;
        if !snapshot {
            let root = self.root.join(".recovery-staging");
            tokio::fs::create_dir_all(&root).await?;
            let meta = tokio::fs::symlink_metadata(&root).await?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(Error::Validation("Invalid recovery staging directory".into()));
            }
            // The guard owns the staging path even when an HTTP request is cancelled.
            let staged = tokio::task::spawn_blocking(move || {
                tempfile::Builder::new()
                    .prefix(&format!(".recover-{}-", id.0))
                    .suffix(".tmp")
                    .tempfile_in(root)
            })
            .await
            .map_err(Error::internal)??;
            let file = tokio::fs::File::from_std(staged.reopen()?);
            let mut input = tokio::fs::File::open(&source).await?.take(plan.size_bytes);
            let mut output = file;
            let copied = tokio::time::timeout(Duration::from_secs(60), async {
                let copied = tokio::io::copy(&mut input, &mut output).await?;
                output.flush().await?;
                output.sync_all().await?;
                Ok::<_, std::io::Error>(copied)
            })
            .await
            .map_err(|_| Error::Unavailable("DVR recovery copy timed out".into()))??;
            if copied != plan.size_bytes {
                return Err(Error::Conflict("Saved capture changed during recovery".into()));
            }
            drop(output);
            let target = self.root.join(format!("{}.recovered.ts", id.0));
            // Atomic no-clobber publication: another recovery cannot expose a half-copy.
            tokio::task::spawn_blocking(move || match staged.persist_noclobber(target) {
                Ok(_) => Ok(()),
                Err(error) if error.error.kind() == ErrorKind::AlreadyExists => Ok(()),
                Err(error) => Err(error.error),
            })
            .await
            .map_err(Error::internal)??;
        }
        self.publish_file_locked(id, None, Some(policy), Some(caller)).await
    }

    pub(super) async fn validate_artifacts(
        &self,
        id: RecordingId,
        stored: Option<&str>,
    ) -> Result<()> {
        if let Some(stored) = stored
            && Path::new(stored) != self.root.join(format!("{}.ts", id.0))
            && Path::new(stored) != self.root.join(format!("{}.recovered.ts", id.0))
        {
            return Err(Error::Validation("Recording path is outside its managed location".into()));
        }
        for name in [format!("{}.ts", id.0), format!("{}.recovered.ts", id.0)] {
            let path = self.root.join(name);
            match self.checked_path(id, path.to_str()).await {
                Ok(_) => {}
                Err(error) if missing(&error) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub(super) async fn remove_artifacts(
        &self,
        id: RecordingId,
        stored: Option<&str>,
    ) -> Result<()> {
        self.validate_artifacts(id, stored).await?;
        for name in [format!("{}.ts", id.0), format!("{}.recovered.ts", id.0)] {
            match tokio::fs::remove_file(self.root.join(name)).await {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        // Remove only this explicitly deleted ID's own recovery scratch files.
        let staging = self.root.join(".recovery-staging");
        if let Ok(meta) = tokio::fs::symlink_metadata(&staging).await
            && (!meta.is_dir() || meta.file_type().is_symlink())
        {
            return Err(Error::Validation("Invalid recovery staging directory".into()));
        }
        match tokio::fs::read_dir(staging).await {
            Ok(mut entries) => {
                while let Some(entry) = entries.next_entry().await? {
                    let name = entry.file_name();
                    let prefix = format!(".recover-{}-", id.0);
                    if name
                        .to_str()
                        .and_then(|n| n.strip_prefix(&prefix))
                        .and_then(|n| n.strip_suffix(".tmp"))
                        .is_some_and(|n| {
                            n.len() == 6 && n.bytes().all(|b| b.is_ascii_alphanumeric())
                        })
                        && entry.file_type().await?.is_file()
                    {
                        tokio::fs::remove_file(entry.path()).await?;
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    pub(super) async fn cleanup_deleted_locked(&self) -> Result<()> {
        let rows =
            m9::deleted_recording_files(&self.db, self.cleanup_cursor.load(Ordering::Relaxed))
                .await?;
        let next = if rows.len() == 200 { rows.last().map_or(0, |r| r.0) } else { 0 };
        for (id, path) in rows {
            match self.remove_artifacts(RecordingId(id), path.as_deref()).await {
                Ok(()) => m9::prune_deleted_recording(&self.db, RecordingId(id), now_ms()).await?,
                Err(error) => {
                    tracing::warn!(recording_id=id,%error,"DVR deleted files retained for cleanup retry")
                }
            }
        }
        self.cleanup_cursor.store(next, Ordering::Relaxed);
        Ok(())
    }
}
