//! DVR file validation, probing and catalogue publication. No arbitrary database paths are opened.
use mbl_core::{Error, ItemId, RecordingId, Result, StreamKind, time::now_ms};
use mbl_db::{
    Db,
    repo::{
        dvr, m9,
        media::{NewStream, VersionSummary},
    },
};
use mbl_probe::FfTools;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::Mutex;

#[path = "dvr_recovery.rs"]
mod recovery;

#[derive(Debug, Clone)]
pub(crate) struct Dvr {
    db: Db,
    root: PathBuf,
    tools: Option<FfTools>,
    // Serializes deletion/publication and limits ffprobe concurrency to one.
    gate: Arc<Mutex<()>>,
    cleanup_cursor: Arc<std::sync::atomic::AtomicI64>,
    pub policy: mbl_core::livetv::DvrStoragePolicy,
}

impl Dvr {
    pub fn new(db: Db, root: PathBuf, tools: Option<FfTools>) -> Self {
        Self {
            db,
            root,
            tools,
            gate: Arc::default(),
            cleanup_cursor: Arc::default(),
            policy: Default::default(),
        }
    }

    pub async fn library_preview(
        &self,
        actor: &mbl_core::Actor,
    ) -> Result<Option<mbl_core::LibraryId>> {
        let root =
            self.root.to_str().ok_or_else(|| Error::Validation("Non-UTF-8 DVR root".into()))?;
        let library = dvr::library_id(&self.db, root).await?;
        match library {
            Some(id) if actor.can_access_library(id) => Ok(Some(id)),
            None if actor.is_admin
                || matches!(actor.library_access, mbl_core::LibraryAccess::All) =>
            {
                Ok(None)
            }
            _ => Err(Error::NotFound),
        }
    }

    pub async fn storage_status(&self) -> mbl_core::livetv::DvrStorageStatus {
        let root = self.root.clone();
        let available =
            tokio::task::spawn_blocking(move || mbl_platform::disk::available_bytes(&root)).await;
        let bytes = match available {
            Ok(Ok(bytes)) => Some(bytes),
            _ => None,
        };
        let can_start = bytes.is_some_and(|n| n > self.policy.min_free_bytes);
        mbl_core::livetv::DvrStorageStatus {
            available_bytes: bytes,
            min_free_bytes: self.policy.min_free_bytes,
            max_recording_bytes: self.policy.max_recording_bytes,
            can_start,
            error: if bytes.is_none() {
                Some("Cannot read DVR filesystem space".into())
            } else if !can_start {
                Some("Insufficient free space for DVR recording".into())
            } else {
                None
            },
        }
    }

    pub async fn library(&self, actor: &mbl_core::Actor) -> Result<mbl_core::LibraryId> {
        let root =
            self.root.to_str().ok_or_else(|| Error::Validation("Non-UTF-8 DVR root".into()))?;
        let existing = dvr::library_id(&self.db, root).await?;
        let library = match existing {
            Some(id) => id,
            None if actor.is_admin
                || matches!(actor.library_access, mbl_core::LibraryAccess::All) =>
            {
                let _guard = self.gate.lock().await;
                tokio::fs::create_dir_all(&self.root).await?;
                dvr::ensure_library(&self.db, root, now_ms()).await?
            }
            None => return Err(Error::NotFound),
        };
        if !actor.can_access_library(library) {
            return Err(Error::NotFound);
        }
        Ok(library)
    }

    pub async fn maintain_retention(&self, playing: &[ItemId]) -> Result<()> {
        // A slow recovery probe must not delay playback admission during retention.
        let Ok(_guard) = self.gate.try_lock() else {
            return Ok(());
        };
        self.cleanup_deleted_locked().await?;
        let mut attempted = 0;
        // The rule owner's watch state decides auto-delete, even in shared libraries.
        let rules = m9::list_recording_rules(&self.db, None).await?;
        for rule in rules {
            let keep_max = rule.keep_max.filter(|n| (1..=10000).contains(n));
            if keep_max.is_none() && rule.auto_delete_watched == 0 {
                continue;
            }
            let rows: Vec<(i64, i64, i64)> =
                m9::retention_rows(&self.db, rule.id, rule.user_id).await?;
            for (index, (id, item, watched)) in rows.into_iter().enumerate() {
                if playing.contains(&ItemId(item)) {
                    continue;
                }
                if keep_max.is_some_and(|max| index >= usize::try_from(max).unwrap_or(usize::MAX))
                    || (rule.auto_delete_watched != 0 && watched != 0)
                {
                    if let Err(error) = self.delete_locked(RecordingId(id)).await {
                        tracing::warn!(recording_id=id,%error,"DVR retention deletion failed");
                    }
                    attempted += 1;
                    if attempted >= 16 {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    async fn checked_path(&self, id: RecordingId, stored: Option<&str>) -> Result<PathBuf> {
        let raw = self.root.join(format!("{}.ts", id.0));
        let archive = self.root.join(format!("{}.recovered.ts", id.0));
        let expected = stored.map_or(raw.clone(), PathBuf::from);
        if expected != raw && expected != archive {
            return Err(Error::Validation("Recording path is outside its managed location".into()));
        }
        let meta = tokio::fs::symlink_metadata(&expected).await?;
        if !meta.is_file() {
            return Err(Error::Validation("Recording is not a regular file".into()));
        }
        let root = dunce::canonicalize(&self.root)?;
        let path = dunce::canonicalize(&expected)?;
        if path.parent() != Some(root.as_path()) {
            return Err(Error::Validation("Recording path escapes its managed location".into()));
        }
        Ok(path)
    }

    pub async fn publish(&self, id: RecordingId) -> Result<ItemId> {
        self.publish_capture(id, None, None).await
    }

    pub async fn publish_capture(
        &self,
        id: RecordingId,
        capture_error: Option<&str>,
        policy: Option<&crate::LiveTvService>,
    ) -> Result<ItemId> {
        let _guard = self.gate.lock().await;
        self.publish_file_locked(id, capture_error, policy, None).await
    }

    async fn publish_file_locked(
        &self,
        id: RecordingId,
        capture_error: Option<&str>,
        policy: Option<&crate::LiveTvService>,
        recovery_caller: Option<&mbl_core::Actor>,
    ) -> Result<ItemId> {
        let recovered = recovery_caller.is_some();
        let row = m9::get_recording(&self.db, id).await?.ok_or(Error::NotFound)?;
        if let Some(item) = row.item_id {
            return Ok(ItemId(item));
        }
        if !matches!(row.status.as_str(), "recording" | "completed" | "partial")
            && !(recovered && row.status == "failed")
        {
            return Err(Error::NotFound);
        }
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| Error::Unavailable("ffprobe is unavailable".into()))?;
        let archive = self.root.join(format!("{}.recovered.ts", id.0));
        let stored = if recovered { archive.to_str() } else { row.output_path.as_deref() };
        let path = self.checked_path(id, stored).await?;
        let before = tokio::fs::metadata(&path).await?;
        let info = tools.probe(&path, Duration::from_secs(15)).await?;
        if info.duration_ms == 0
            || (info.primary_video().is_none()
                && info.streams_of(StreamKind::Audio).next().is_none())
            || before.len() == 0
        {
            return Err(Error::Validation(
                "Recording has no playable audio/video or duration".into(),
            ));
        }
        let checked = self.checked_path(id, stored).await?;
        let after = tokio::fs::metadata(&checked).await?;
        if checked != path
            || before.len() != after.len()
            || before.modified()? != after.modified()?
        {
            return Err(Error::Conflict("Recording changed during probing".into()));
        }
        let incomplete = if recovered {
            Some(
                "Incomplete recording: recovered saved media after a failed or interrupted capture"
                    .to_owned(),
            )
        } else if row.status == "partial" {
            Some(row.error_message.clone().unwrap_or_else(|| "Incomplete recording".into()))
        } else if row.status == "recording" {
            incomplete_reason(row.start_time, row.end_time, info.duration_ms, capture_error)
        } else {
            None // Legacy completed archives lack reliable capture accounting.
        };
        let video = info.primary_video();
        let audio = info
            .streams_of(StreamKind::Audio)
            .find(|s| s.default)
            .or_else(|| info.streams_of(StreamKind::Audio).next());
        let hdr = video.and_then(|s| s.hdr).map(|hdr| match hdr {
            mbl_probe::HdrKind::Hdr10 => "hdr10".to_owned(),
            mbl_probe::HdrKind::Hdr10Plus => "hdr10plus".to_owned(),
            mbl_probe::HdrKind::Hlg => "hlg".to_owned(),
            mbl_probe::HdrKind::DolbyVision { profile, .. } => format!("dv{profile}"),
        });
        let probe = serde_json::to_string(&info).map_err(Error::internal)?;
        let chapters = serde_json::to_string(&info.chapters).map_err(Error::internal)?;
        // Store the configured spelling of the root/path, as used by the file writer.
        let path_string = self.root.join(path.file_name().ok_or(Error::NotFound)?);
        let root =
            self.root.to_str().ok_or_else(|| Error::Validation("Non-UTF-8 DVR root".into()))?;
        let path_string =
            path_string.to_str().ok_or_else(|| Error::Validation("Non-UTF-8 DVR path".into()))?;
        let media = dvr::RecordedMedia {
            recovered,
            incomplete_reason: incomplete.as_deref(),
            root,
            path: path_string,
            size: i64::try_from(after.len()).map_err(Error::internal)?,
            mtime: after
                .modified()?
                .duration_since(UNIX_EPOCH)
                .map_err(Error::internal)?
                .as_millis()
                .try_into()
                .map_err(Error::internal)?,
            duration_ms: info.duration_ms.try_into().map_err(Error::internal)?,
            probe: &probe,
            chapters: &chapters,
            version: VersionSummary {
                container: Some(&info.container),
                bitrate: info.bitrate.and_then(|b| b.try_into().ok()),
                width: video.and_then(|s| s.width).map(i64::from),
                height: video.and_then(|s| s.height).map(i64::from),
                video_codec: video.map(|s| s.codec.as_str()),
                video_profile: video.and_then(|s| s.profile.as_deref()),
                bit_depth: video.and_then(|s| s.bit_depth).map(i64::from),
                hdr: hdr.as_deref(),
                audio_codec: audio.map(|s| s.codec.as_str()),
                audio_channels: audio.and_then(|s| s.channels).map(i64::from),
                ..Default::default()
            },
            streams: info
                .streams
                .iter()
                .map(|s| NewStream {
                    idx: i64::from(s.index),
                    kind: match s.kind {
                        StreamKind::Video => "video",
                        StreamKind::Audio => "audio",
                        StreamKind::Subtitle => "subtitle",
                        StreamKind::Attachment => "attachment",
                    },
                    codec: &s.codec,
                    profile: s.profile.as_deref(),
                    language: s.language.as_deref(),
                    title: s.title.as_deref(),
                    is_default: s.default,
                    is_forced: s.forced,
                    width: s.width.map(i64::from),
                    height: s.height.map(i64::from),
                    bit_depth: s.bit_depth.map(i64::from),
                    channels: s.channels.map(i64::from),
                    channel_layout: s.channel_layout.as_deref(),
                    ..Default::default()
                })
                .collect(),
        };
        // A profile/guide change during a slow probe must not publish stale ratings.
        if let Some(policy) = policy {
            let latest = m9::get_recording(&self.db, id).await?.ok_or(Error::NotFound)?;
            if let Some(caller) = recovery_caller {
                policy.authorize_recording_recovery(caller, id).await?;
            } else {
                policy.authorize_recording_job(&latest).await?;
            }
        }
        dvr::publish(&self.db, id, &media, now_ms()).await
    }

    pub async fn reconcile(&self) {
        {
            let _guard = self.gate.lock().await;
            if let Err(error) = self.cleanup_deleted_locked().await {
                tracing::warn!(%error,"DVR deleted-file recovery failed");
            }
        }
        match m9::list_recordings(&self.db, None).await {
            Ok(rows) => {
                for row in rows.into_iter().filter(|row| {
                    row.item_id.is_none() && matches!(row.status.as_str(), "completed" | "partial")
                }) {
                    if let Err(error) = self.publish(RecordingId(row.id)).await {
                        tracing::warn!(recording_id=row.id, %error, "DVR catalogue recovery failed");
                    }
                }
            }
            Err(error) => tracing::error!(%error, "DVR catalogue recovery read failed"),
        }
    }

    pub async fn delete(&self, id: RecordingId) -> Result<bool> {
        let _guard = self.gate.lock().await;
        self.delete_locked(id).await
    }

    async fn delete_locked(&self, id: RecordingId) -> Result<bool> {
        let Some(row) = m9::get_recording(&self.db, id).await? else {
            return Ok(false);
        };
        self.validate_artifacts(id, row.output_path.as_deref()).await?;
        // Commit deletion before unlinking. Its tombstone is the durable cleanup queue.
        let deleted = m9::delete_recording(&self.db, id).await?;
        if let Err(error) = self.remove_artifacts(id, row.output_path.as_deref()).await {
            tracing::warn!(recording_id=id.0,%error,"DVR deletion committed; file cleanup will retry");
        }
        Ok(deleted)
    }
}

fn incomplete_reason(start: i64, end: i64, duration: u64, failure: Option<&str>) -> Option<String> {
    if let Some(error) = failure {
        return Some(format!("Incomplete recording: {error}"));
    }
    let expected = u64::try_from(end.saturating_sub(start)).unwrap_or(0);
    // Stream-copy packet boundaries and scheduler jitter need a small tolerance.
    // Cap it at 2 seconds and 5% of short programmes (minimum 100 ms).
    let tolerance = (expected / 20).clamp(100, 2000);
    (duration.saturating_add(tolerance) < expected).then(|| {
        format!(
            "Incomplete recording: captured {:.1} s of {:.1} s scheduled",
            duration as f64 / 1000.0,
            expected as f64 / 1000.0,
        )
    })
}
