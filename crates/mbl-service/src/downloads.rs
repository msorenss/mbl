//! Offline downloads for client devices (M11.1 S4).
//!
//! A download is a per-user ticket for one media part in a chosen quality. `original`
//! serves the source file as-is; optimized qualities are transcoded once into the cache
//! (`<cache>/downloads/<part>-<quality>.mp4`) by a background job and shared between users.
//! Every file request re-checks library access and content rating.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use mbl_analysis::{OptimizationProfile, build_optimize_ffmpeg_args};
use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, ItemId, PartId, Result, UserId};
use mbl_db::Db;
use mbl_db::repo::downloads::{self, DownloadOutcome, DownloadRow};
use mbl_db::repo::items::{self, ItemFilter};
use mbl_db::repo::media;
use mbl_jobs::{JobCtx, JobHandler, JobQueue, Priority, kinds};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use utoipa::ToSchema;

use crate::stream::StreamService;

const QUALITY_ORIGINAL: &str = "original";
const MAX_ERROR_LEN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadQuality {
    /// The source file, unchanged.
    Original,
    /// H.264/AAC MP4, at most 1920 px wide.
    #[serde(rename = "universal_1080p")]
    Universal1080p,
    /// H.264/AAC MP4, at most 1280 px wide.
    #[serde(rename = "universal_720p")]
    Universal720p,
}

impl DownloadQuality {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Original => QUALITY_ORIGINAL,
            Self::Universal1080p => "universal_1080p",
            Self::Universal720p => "universal_720p",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            QUALITY_ORIGINAL => Some(Self::Original),
            "universal_1080p" => Some(Self::Universal1080p),
            "universal_720p" => Some(Self::Universal720p),
            _ => None,
        }
    }

    fn profile(self) -> Option<OptimizationProfile> {
        match self {
            Self::Original => None,
            Self::Universal1080p => Some(OptimizationProfile::Universal1080p),
            Self::Universal720p => Some(OptimizationProfile::Universal720p),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// Waiting for the transcode job.
    Pending,
    /// The server is transcoding the file.
    Transcoding,
    /// The file can be fetched from `/api/v1/downloads/{id}/file`.
    Ready,
    /// Transcoding failed; see `error`. Re-create the download to retry.
    Failed,
}

impl DownloadStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Transcoding => "transcoding",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "transcoding" => Self::Transcoding,
            "ready" => Self::Ready,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateDownloadRequest {
    pub quality: DownloadQuality,
    /// Part to download; defaults to the item's first part.
    #[serde(default)]
    pub part_id: Option<PartId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDto {
    pub id: i64,
    pub item_id: ItemId,
    pub part_id: PartId,
    pub quality: DownloadQuality,
    pub status: DownloadStatus,
    /// File size in bytes once known.
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
    /// Suggested file name for the client's local copy.
    pub file_name: String,
    /// MIME type of the downloaded file.
    pub mime_type: String,
    /// Unix milliseconds.
    pub created_at: i64,
    /// Unix milliseconds.
    pub updated_at: i64,
}

/// A download file resolved for serving.
#[derive(Debug, Clone)]
pub struct DownloadFile {
    pub path: PathBuf,
    pub container: Option<String>,
    pub file_name: String,
}

#[derive(Debug, Clone)]
pub struct DownloadService {
    db: Db,
    jobs: JobQueue,
    stream: StreamService,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TranscodePayload {
    part_id: PartId,
    quality: String,
}

impl DownloadService {
    pub fn new(db: Db, jobs: JobQueue, stream: StreamService) -> Self {
        Self { db, jobs, stream }
    }

    pub fn with_stream(mut self, stream: StreamService) -> Self {
        self.stream = stream;
        self
    }

    /// Create (or return the existing) download ticket for an item the actor can play.
    pub async fn create(
        &self,
        actor: &Actor,
        item_id: ItemId,
        req: CreateDownloadRequest,
    ) -> Result<DownloadDto> {
        items::detail(&self.db, &ItemFilter::for_user(actor), item_id)
            .await?
            .ok_or(Error::NotFound)?;
        let parts = media::parts_for_item(&self.db, item_id).await?;
        let part = match req.part_id {
            Some(id) => parts.into_iter().find(|p| p.id == id),
            None => parts.into_iter().next(),
        }
        .ok_or(Error::NotFound)?;
        // Re-checks library access, rating and that the file lies under a library root.
        let resolved = self.stream.resolve_part(actor, part.id).await?;
        let quality = req.quality;
        let now = now_ms();

        let Some(profile) = quality.profile() else {
            let size = tokio::fs::metadata(&resolved.path)
                .await
                .map(|m| i64::try_from(m.len()).unwrap_or(i64::MAX))
                .map_err(|_| Error::NotFound)?;
            let (row, created) = downloads::insert_or_get(
                &self.db,
                actor.user_id,
                item_id,
                part.id,
                QUALITY_ORIGINAL,
                DownloadStatus::Ready.as_str(),
                None,
                Some(size),
                now,
            )
            .await?;
            if !created {
                downloads::set_status(&self.db, row.id, "ready", None, Some(size), None, now)
                    .await?;
            }
            return self.load_dto(row.id).await;
        };

        let target = self.cache_target(part.id, profile)?;
        let reuse = downloads::find_ready_file(&self.db, part.id, quality.as_str())
            .await?
            .filter(|(p, _)| Path::new(p) == target && target.is_file());
        let (status, file, size) = match &reuse {
            Some((p, s)) => (DownloadStatus::Ready, Some(p.as_str()), Some(*s)),
            None => (DownloadStatus::Pending, None, None),
        };
        let (row, created) = downloads::insert_or_get(
            &self.db,
            actor.user_id,
            item_id,
            part.id,
            quality.as_str(),
            status.as_str(),
            file,
            size,
            now,
        )
        .await?;
        let current = DownloadStatus::parse(&row.status);
        let needs_job = if created {
            status == DownloadStatus::Pending
        } else if current == DownloadStatus::Failed
            || (current == DownloadStatus::Ready
                && !row.file_path.as_deref().is_some_and(|p| Path::new(p).is_file()))
        {
            // Retry a failed ticket or one whose cached file vanished.
            match &reuse {
                Some((p, s)) => {
                    downloads::set_status(&self.db, row.id, "ready", Some(p), Some(*s), None, now)
                        .await?;
                    false
                }
                None => {
                    downloads::set_status(&self.db, row.id, "pending", None, None, None, now)
                        .await?;
                    true
                }
            }
        } else {
            current == DownloadStatus::Pending
        };
        if needs_job {
            let payload = serde_json::to_value(TranscodePayload {
                part_id: part.id,
                quality: quality.as_str().to_string(),
            })
            .map_err(Error::internal)?;
            self.jobs.enqueue(kinds::DOWNLOAD_TRANSCODE, &payload, Priority::Normal).await?;
        }
        self.load_dto(row.id).await
    }

    pub async fn list(&self, actor: &Actor) -> Result<Vec<DownloadDto>> {
        let rows = downloads::list_for_user(&self.db, actor.user_id).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(self.to_dto(row).await?);
        }
        Ok(out)
    }

    pub async fn get(&self, actor: &Actor, id: i64) -> Result<DownloadDto> {
        let row = self.owned(actor.user_id, id).await?;
        self.to_dto(row).await
    }

    /// Delete a ticket; its cached transcode is removed once no ticket references it.
    pub async fn delete(&self, actor: &Actor, id: i64) -> Result<()> {
        let row = self.owned(actor.user_id, id).await?;
        downloads::delete(&self.db, row.id).await?;
        if let Some(path) = row.file_path.as_deref() {
            self.remove_unreferenced(path).await?;
        }
        Ok(())
    }

    /// Resolve the file behind a ready ticket, re-checking current access.
    pub async fn resolve_file(&self, actor: &Actor, id: i64) -> Result<DownloadFile> {
        let row = self.owned(actor.user_id, id).await?;
        let resolved = self.stream.resolve_part(actor, row.part_id).await?;
        let quality = DownloadQuality::parse(&row.quality).ok_or(Error::NotFound)?;
        if DownloadStatus::parse(&row.status) != DownloadStatus::Ready {
            return Err(Error::Conflict("download is not ready yet".into()));
        }
        let file_name = file_name_for(&resolved.path, quality);
        if quality == DownloadQuality::Original {
            return Ok(DownloadFile {
                path: resolved.path,
                container: resolved.container,
                file_name,
            });
        }
        let path = PathBuf::from(row.file_path.ok_or(Error::NotFound)?);
        if !path.is_file() {
            return Err(Error::NotFound);
        }
        Ok(DownloadFile { path, container: Some("mp4".into()), file_name })
    }

    async fn owned(&self, user_id: UserId, id: i64) -> Result<DownloadRow> {
        downloads::get(&self.db, id).await?.filter(|r| r.user_id == user_id).ok_or(Error::NotFound)
    }

    async fn load_dto(&self, id: i64) -> Result<DownloadDto> {
        let row = downloads::get(&self.db, id).await?.ok_or(Error::NotFound)?;
        self.to_dto(row).await
    }

    async fn to_dto(&self, row: DownloadRow) -> Result<DownloadDto> {
        let quality = DownloadQuality::parse(&row.quality).unwrap_or(DownloadQuality::Original);
        let part = media::part_for_stream(&self.db, row.part_id).await?;
        let file_name = match &part {
            Some(p) => file_name_for(Path::new(&p.path), quality),
            None => format!("download-{}.bin", row.id),
        };
        let mime_type = match quality {
            DownloadQuality::Original => mime_for(Path::new(&file_name)).to_string(),
            _ => "video/mp4".to_string(),
        };
        Ok(DownloadDto {
            id: row.id,
            item_id: row.item_id,
            part_id: row.part_id,
            quality,
            status: DownloadStatus::parse(&row.status),
            size_bytes: row.size_bytes,
            error: row.error,
            file_name,
            mime_type,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    fn downloads_dir(&self) -> Result<PathBuf> {
        self.stream
            .cache_dir()
            .map(|c| c.join("downloads"))
            .ok_or_else(|| Error::Unavailable("transcoded downloads need a cache directory".into()))
    }

    fn cache_target(&self, part_id: PartId, profile: OptimizationProfile) -> Result<PathBuf> {
        Ok(self.downloads_dir()?.join(format!("{}-{}.mp4", part_id.0, profile.as_str())))
    }

    async fn remove_unreferenced(&self, path: &str) -> Result<()> {
        let Ok(dir) = self.downloads_dir() else { return Ok(()) };
        // Never touch files outside the managed downloads directory.
        if Path::new(path).parent() != Some(dir.as_path()) {
            return Ok(());
        }
        if downloads::count_file_refs(&self.db, path).await? == 0 {
            let _ = tokio::fs::remove_file(path).await;
        }
        Ok(())
    }

    async fn run_transcode(&self, ctx: &JobCtx, payload: TranscodePayload) -> Result<()> {
        let Some(quality) = DownloadQuality::parse(&payload.quality) else {
            return Ok(());
        };
        let Some(profile) = quality.profile() else { return Ok(()) };
        let target = self.cache_target(payload.part_id, profile)?;
        let target_str = target.to_string_lossy().into_owned();
        let quality_str = quality.as_str();
        let now = now_ms();

        if target.is_file() {
            let size = file_size(&target).await;
            self.finish(payload.part_id, quality_str, "ready", Some(&target_str), size, None)
                .await?;
            return Ok(());
        }
        let marked = downloads::complete_pending(
            &self.db,
            payload.part_id,
            quality_str,
            DownloadOutcome { status: "transcoding", ..Default::default() },
            now,
        )
        .await?;
        if marked == 0 {
            return Ok(()); // every ticket was deleted meanwhile
        }
        ctx.progress.report(0.05).await?;

        // Path checks only; per-user access was verified when the ticket was created and is
        // verified again on every file request.
        let resolved =
            match self.stream.resolve_part(&Actor::admin(UserId(0)), payload.part_id).await {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("source file unavailable: {e}");
                    self.finish(payload.part_id, quality_str, "failed", None, None, Some(&msg))
                        .await?;
                    return Ok(());
                }
            };
        let partial = target.with_extension("partial.mp4");
        match self.ffmpeg(ctx, &resolved.path, &partial, profile).await {
            Ok(()) => {
                tokio::fs::rename(&partial, &target).await.map_err(Error::internal)?;
                let size = file_size(&target).await;
                let updated = self
                    .finish(payload.part_id, quality_str, "ready", Some(&target_str), size, None)
                    .await?;
                if updated == 0 {
                    self.remove_unreferenced(&target_str).await?;
                }
                ctx.progress.report(1.0).await?;
                Ok(())
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&partial).await;
                if ctx.cancel.is_cancelled() {
                    // Shutdown or cancel: leave tickets pending so the job can run again.
                    downloads::complete_pending(
                        &self.db,
                        payload.part_id,
                        quality_str,
                        DownloadOutcome { status: "pending", ..Default::default() },
                        now_ms(),
                    )
                    .await?;
                    return Err(e);
                }
                let msg = e.to_string();
                self.finish(payload.part_id, quality_str, "failed", None, None, Some(&msg)).await?;
                Ok(())
            }
        }
    }

    async fn finish(
        &self,
        part_id: PartId,
        quality: &str,
        status: &str,
        file: Option<&str>,
        size: Option<i64>,
        error: Option<&str>,
    ) -> Result<u64> {
        let error = error.map(|e| truncate(e, MAX_ERROR_LEN));
        downloads::complete_pending(
            &self.db,
            part_id,
            quality,
            DownloadOutcome { status, file_path: file, size_bytes: size, error: error.as_deref() },
            now_ms(),
        )
        .await
    }

    async fn ffmpeg(
        &self,
        ctx: &JobCtx,
        input: &Path,
        output: &Path,
        profile: OptimizationProfile,
    ) -> Result<()> {
        if let Some(parent) = output.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(Error::internal)?;
        }
        let mut child = tokio::process::Command::new(self.stream.ffmpeg_bin())
            .args(build_optimize_ffmpeg_args(input, output, profile))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(Error::internal)?;
        let mut stderr = child.stderr.take();
        let read_stderr = async move {
            let mut buf = Vec::new();
            if let Some(s) = stderr.as_mut() {
                let _ = s.take(64 * 1024).read_to_end(&mut buf).await;
            }
            buf
        };
        let (status, stderr) = tokio::select! {
            res = async { tokio::join!(child.wait(), read_stderr) } => res,
            () = ctx.cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(Error::Unavailable("download transcode cancelled".into()));
            }
        };
        let status = status.map_err(Error::internal)?;
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr);
            return Err(Error::Internal(
                format!("FFmpeg transcode failed ({status}): {}", stderr.trim()).into(),
            ));
        }
        Ok(())
    }
}

async fn file_size(path: &Path) -> Option<i64> {
    tokio::fs::metadata(path).await.ok().map(|m| i64::try_from(m.len()).unwrap_or(i64::MAX))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn file_name_for(source: &Path, quality: DownloadQuality) -> String {
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("download");
    match quality {
        DownloadQuality::Original => source
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{stem}.bin")),
        DownloadQuality::Universal1080p => format!("{stem}.1080p.mp4"),
        DownloadQuality::Universal720p => format!("{stem}.720p.mp4"),
    }
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("avi") => "video/x-msvideo",
        Some("mov") => "video/quicktime",
        Some("ts" | "m2ts") => "video/mp2t",
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("m4a") => "audio/mp4",
        Some("ogg" | "oga") => "audio/ogg",
        Some("opus") => "audio/opus",
        Some("wav") => "audio/wav",
        _ => "application/octet-stream",
    }
}

/// Background job handler for `download.transcode`.
pub struct DownloadJobHandler {
    pub downloads: DownloadService,
}

#[async_trait]
impl JobHandler for DownloadJobHandler {
    fn kind(&self) -> &'static str {
        kinds::DOWNLOAD_TRANSCODE
    }

    async fn run(&self, ctx: JobCtx, payload: serde_json::Value) -> Result<()> {
        let payload: TranscodePayload =
            serde_json::from_value(payload).map_err(|e| Error::Validation(e.to_string()))?;
        self.downloads.run_transcode(&ctx, payload).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_round_trips() {
        for q in [
            DownloadQuality::Original,
            DownloadQuality::Universal1080p,
            DownloadQuality::Universal720p,
        ] {
            assert_eq!(DownloadQuality::parse(q.as_str()), Some(q));
            let json = serde_json::to_value(q).unwrap();
            assert_eq!(json, q.as_str(), "API and storage names must match");
        }
        assert_eq!(DownloadQuality::parse("4k"), None);
    }

    #[test]
    fn file_names_follow_quality() {
        let p = Path::new("/media/Movies/Film (2020).mkv");
        assert_eq!(file_name_for(p, DownloadQuality::Original), "Film (2020).mkv");
        assert_eq!(file_name_for(p, DownloadQuality::Universal720p), "Film (2020).720p.mp4");
        assert_eq!(mime_for(Path::new("a.MKV")), "video/x-matroska");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("åäö", 3), "å…");
        assert_eq!(truncate("abc", 5), "abc");
    }
}
