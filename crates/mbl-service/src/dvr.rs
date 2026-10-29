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
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub(crate) struct Dvr {
    db: Db,
    root: PathBuf,
    tools: Option<FfTools>,
    // Serializes deletion/publication and limits ffprobe concurrency to one.
    gate: Arc<Mutex<()>>,
}

impl Dvr {
    pub fn new(db: Db, root: PathBuf, tools: Option<FfTools>) -> Self {
        Self { db, root, tools, gate: Arc::default() }
    }

    async fn checked_path(&self, id: RecordingId, stored: Option<&str>) -> Result<PathBuf> {
        let expected = self.root.join(format!("{}.ts", id.0));
        if stored.is_some_and(|p| Path::new(p) != expected) {
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
        let _guard = self.gate.lock().await;
        let row = m9::get_recording(&self.db, id).await?.ok_or(Error::NotFound)?;
        if let Some(item) = row.item_id {
            return Ok(ItemId(item));
        }
        if row.status != "recording" && row.status != "completed" {
            return Err(Error::NotFound);
        }
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| Error::Unavailable("ffprobe is unavailable".into()))?;
        let path = self.checked_path(id, row.output_path.as_deref()).await?;
        let before = tokio::fs::metadata(&path).await?;
        let info = tools.probe(&path, Duration::from_secs(15)).await?;
        if info.duration_ms == 0 || info.primary_video().is_none() || before.len() == 0 {
            return Err(Error::Validation("Recording has no playable video or duration".into()));
        }
        let checked = self.checked_path(id, row.output_path.as_deref()).await?;
        let after = tokio::fs::metadata(&checked).await?;
        if checked != path
            || before.len() != after.len()
            || before.modified()? != after.modified()?
        {
            return Err(Error::Conflict("Recording changed during probing".into()));
        }
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
        let path_string = self.root.join(format!("{}.ts", id.0));
        let root =
            self.root.to_str().ok_or_else(|| Error::Validation("Non-UTF-8 DVR root".into()))?;
        let path_string =
            path_string.to_str().ok_or_else(|| Error::Validation("Non-UTF-8 DVR path".into()))?;
        let media = dvr::RecordedMedia {
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
        dvr::publish(&self.db, id, &media, now_ms()).await
    }

    pub async fn reconcile(&self) {
        match m9::list_recordings(&self.db, Some("completed")).await {
            Ok(rows) => {
                for row in rows.into_iter().filter(|row| row.item_id.is_none()) {
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
        let Some(row) = m9::get_recording(&self.db, id).await? else {
            return Ok(false);
        };
        if row.status != "scheduled" && row.status != "recording" {
            // Missing files may still have their obsolete catalogue entries removed.
            match self.checked_path(id, row.output_path.as_deref()).await {
                Ok(path) => tokio::fs::remove_file(path).await?,
                Err(Error::Internal(e))
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        m9::delete_recording(&self.db, id).await
    }
}
