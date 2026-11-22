//! Direct-play, HLS session lookup, and media part resolution with library ACL and path-traversal enforcement.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use mbl_core::{Actor, Error, ItemId, LibraryId, PartId, Result, StreamKind};
use mbl_db::Db;
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::media;
use mbl_stream::subtitles::{
    SubtitleFormat, SubtitleSource, cached_vtt_path, extract_embedded_vtt,
    extract_or_convert_subtitle,
};
use mbl_transcode::{HlsSession, SessionManager};

/// Verified on-disk media part ready to be streamed.
#[derive(Debug, Clone)]
pub struct ResolvedPart {
    pub id: PartId,
    pub item_id: ItemId,
    pub library_id: LibraryId,
    pub path: PathBuf,
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    pub probe: String,
}

#[derive(Debug, Clone)]
pub struct StreamService {
    db: Db,
    libraries: LibrariesRepo,
    hls: Option<SessionManager>,
    cache_dir: Option<PathBuf>,
    ffmpeg_bin: PathBuf,
}

impl StreamService {
    pub fn new(db: Db) -> Self {
        Self {
            libraries: LibrariesRepo::new(db.clone()),
            db,
            hls: None,
            cache_dir: None,
            ffmpeg_bin: PathBuf::from("ffmpeg"),
        }
    }

    pub fn with_hls(mut self, hls: SessionManager) -> Self {
        self.ffmpeg_bin = hls.config().ffmpeg_bin.clone();
        self.hls = Some(hls);
        self
    }

    pub fn with_cache_dir(mut self, cache_dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self
    }

    pub fn ffmpeg_bin(&self) -> &std::path::Path {
        &self.ffmpeg_bin
    }

    pub fn with_ffmpeg_bin(mut self, ffmpeg_bin: impl Into<PathBuf>) -> Self {
        self.ffmpeg_bin = ffmpeg_bin.into();
        self
    }

    pub fn hls_manager(&self) -> Option<&SessionManager> {
        self.hls.as_ref()
    }

    pub fn cache_dir(&self) -> Option<&Path> {
        self.cache_dir.as_deref()
    }

    pub fn hls_session(&self, session_id: &str) -> Result<Arc<HlsSession>> {
        let mgr = self
            .hls
            .as_ref()
            .ok_or_else(|| Error::Unavailable("HLS transcode manager is not configured".into()))?;
        mgr.get_session(session_id).ok_or(Error::NotFound)
    }

    pub async fn hls_master_playlist(&self, session_id: &str) -> Result<String> {
        let session = self.hls_session(session_id)?;
        session.request_master_playlist().await
    }

    pub async fn hls_media_playlist(&self, session_id: &str, rendition: &str) -> Result<String> {
        let session = self.hls_session(session_id)?;
        session.request_media_playlist(rendition).await
    }

    pub async fn hls_init_segment(&self, session_id: &str, rendition: &str) -> Result<PathBuf> {
        let session = self.hls_session(session_id)?;
        session.request_init(rendition).await
    }

    pub async fn hls_media_segment(
        &self,
        session_id: &str,
        rendition: &str,
        segment_num: u32,
    ) -> Result<PathBuf> {
        let session = self.hls_session(session_id)?;
        session.request_segment(rendition, segment_num).await
    }

    /// Resolve `part_id` for `actor`, enforcing library ACLs and verifying that the resolved
    /// file path resides inside one of the owning library's configured roots.
    pub async fn resolve_part(&self, actor: &Actor, part_id: PartId) -> Result<ResolvedPart> {
        let part = media::part_for_stream(&self.db, part_id).await?.ok_or(Error::NotFound)?;
        if !actor.can_access_library(part.library_id) {
            return Err(Error::NotFound);
        }
        let rating = mbl_db::repo::m6::get_item_content_rating(&self.db, part.item_id).await?;
        if !actor.can_view_rating(rating.as_deref()) {
            return Err(Error::NotFound);
        }
        let lib = self.libraries.get(part.library_id).await?.ok_or(Error::NotFound)?;
        let path = verify_under_library_roots(&part.path, &lib.paths)?;
        Ok(ResolvedPart {
            id: part.id,
            item_id: part.item_id,
            library_id: part.library_id,
            path,
            container: part.container,
            duration_ms: part.duration_ms,
            probe: part.probe,
        })
    }

    /// FFmpeg input for a verified part path: the path itself, or a signed URL (valid 12 hours)
    /// for files on a storage node.
    pub async fn media_input(&self, path: &Path) -> Result<PathBuf> {
        match path.to_str() {
            Some(raw) if is_node_path(raw) => Ok(PathBuf::from(
                crate::nodes::signed_node_file_url(&self.db, raw, 12 * 3600).await?,
            )),
            _ => Ok(path.to_path_buf()),
        }
    }

    /// Serve a resolved part with Range support: from disk, or relayed from its storage node.
    pub async fn serve_part(
        &self,
        part: &ResolvedPart,
        range: Option<&str>,
        head: bool,
    ) -> Result<axum::response::Response> {
        match part.path.to_str() {
            Some(raw) if is_node_path(raw) => {
                crate::nodes::relay_node_file(&self.db, raw, range, head).await
            }
            _ => {
                mbl_stream::serve_direct_file(&part.path, part.container.as_deref(), range, head)
                    .await
            }
        }
    }

    /// Resolve, convert (if needed), and cache a subtitle stream (`.vtt` or `.ass`) for `part_id`.
    pub async fn resolve_subtitle(
        &self,
        actor: &Actor,
        part_id: PartId,
        stream_index: u32,
        format: SubtitleFormat,
        match_speech: bool,
    ) -> Result<PathBuf> {
        let part = media::part_for_stream(&self.db, part_id).await?.ok_or(Error::NotFound)?;
        if !actor.can_access_library(part.library_id) {
            return Err(Error::NotFound);
        }
        let rating = mbl_db::repo::m6::get_item_content_rating(&self.db, part.item_id).await?;
        if !actor.can_view_rating(rating.as_deref()) {
            return Err(Error::NotFound);
        }
        let lib = self.libraries.get(part.library_id).await?.ok_or(Error::NotFound)?;

        let db_streams = media::streams_for_part(&self.db, part_id).await?;
        let found: Option<(StreamKind, String, Option<String>)> = db_streams
            .into_iter()
            .find(|s| s.idx == i64::from(stream_index))
            .map(|s| (s.kind, s.codec, s.external_path))
            .or_else(|| {
                let info: mbl_probe::MediaInfo = serde_json::from_str(&part.probe).ok()?;
                info.streams
                    .into_iter()
                    .find(|s| s.index == stream_index)
                    .map(|s| (s.kind, s.codec, s.external_path))
            });

        let (kind, codec, external_path) = found.ok_or(Error::NotFound)?;
        if kind != StreamKind::Subtitle {
            return Err(Error::NotFound);
        }

        let source = match external_path {
            Some(ext) => {
                let path = verify_under_library_roots(&ext, &lib.paths)?;
                SubtitleSource::External { path, codec }
            }
            None => {
                let media_path = verify_under_library_roots(&part.path, &lib.paths)?;
                let media_path = self.media_input(&media_path).await?;
                SubtitleSource::Embedded { media_path, stream_index, codec }
            }
        };

        let cache_dir = self.subtitle_cache_dir();
        // A text track inside the file: take out all of them in one read of the file.
        if format == SubtitleFormat::Vtt
            && matches!(source, SubtitleSource::Embedded { .. })
            && !tokio::fs::metadata(cached_vtt_path(&cache_dir, part_id, stream_index))
                .await
                .is_ok_and(|m| m.len() > 0)
            && let Err(error) = self.prepare_subtitles(part_id, true).await
        {
            tracing::warn!(%error, part = part_id.0, "subtitle batch failed; extracting one track");
        }
        let ffmpeg_bin = self
            .hls
            .as_ref()
            .map(|h| h.config().ffmpeg_bin.as_path())
            .unwrap_or(self.ffmpeg_bin.as_path());

        let path = extract_or_convert_subtitle(
            &cache_dir,
            ffmpeg_bin,
            part_id,
            stream_index,
            &source,
            format,
        )
        .await?;
        if !match_speech || format != SubtitleFormat::Vtt {
            return Ok(path);
        }
        // Moving the subtitles to the speech is a help, never a reason to show none.
        match self.synced_vtt(part_id, stream_index, &path).await {
            Ok(synced) => Ok(synced),
            Err(error) => {
                tracing::warn!(%error, part = part_id.0, "subtitles could not be matched to speech");
                Ok(path)
            }
        }
    }

    fn subtitle_cache_dir(&self) -> PathBuf {
        self.cache_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("mbl-subtitle-cache"))
    }

    /// Extract every embedded text subtitle track of `part_id` to WebVTT in one read of the file,
    /// skipping tracks already cached, and in the same read map where people speak (for
    /// [`Self::speech_map`]) unless that is cached too. Returns how many tracks were written. No
    /// access check: for background jobs and callers that have already checked. With `detached`
    /// the work runs in its own task, so a viewer who gives up waiting does not throw it away;
    /// otherwise dropping the future stops FFmpeg (a background job that gives way to playback).
    pub async fn prepare_subtitles(&self, part_id: PartId, detached: bool) -> Result<usize> {
        let part = media::part_for_stream(&self.db, part_id).await?.ok_or(Error::NotFound)?;
        let streams = media::streams_for_part(&self.db, part_id).await?;
        let text: Vec<_> = streams
            .iter()
            .filter(|s| s.kind == StreamKind::Subtitle)
            .filter(|s| !mbl_stream::subtitles::is_image_subtitle(&s.codec))
            .collect();
        let indices: Vec<u32> = text
            .iter()
            .filter(|s| s.external_path.is_none())
            .filter_map(|s| u32::try_from(s.idx).ok())
            .collect();
        let cache_dir = self.subtitle_cache_dir();
        let speech_file = speech_map_path(&cache_dir, part_id);
        let want_speech = !text.is_empty()
            && streams.iter().any(|s| s.kind == StreamKind::Audio)
            && !tokio::fs::try_exists(&speech_file).await.unwrap_or(false);
        if indices.is_empty() && !want_speech {
            return Ok(0);
        }
        let lib = self.libraries.get(part.library_id).await?.ok_or(Error::NotFound)?;
        let media_path = verify_under_library_roots(&part.path, &lib.paths)?;
        let media_path = self.media_input(&media_path).await?;
        let ffmpeg_bin = self
            .hls
            .as_ref()
            .map(|h| h.config().ffmpeg_bin.clone())
            .unwrap_or_else(|| self.ffmpeg_bin.clone());
        let work = async move {
            let mut detector = mbl_analysis::subsync::SpeechDetector::new();
            let written = {
                let mut feed = |chunk: &[u8]| detector.feed(chunk);
                let audio: Option<&mut mbl_stream::subtitles::AudioSink<'_>> =
                    if want_speech { Some(&mut feed) } else { None };
                extract_embedded_vtt(&cache_dir, &ffmpeg_bin, part_id, &media_path, &indices, audio)
                    .await?
            };
            if want_speech {
                let speech = detector.finish();
                if !speech.is_empty() {
                    let tmp = speech_file.with_extension("speech.tmp");
                    let packed = mbl_analysis::subsync::pack_speech(&speech);
                    if tokio::fs::write(&tmp, packed).await.is_ok() {
                        let _ = tokio::fs::rename(&tmp, &speech_file).await;
                    }
                }
            }
            Ok(written)
        };
        if detached { tokio::spawn(work).await.map_err(Error::internal)? } else { work.await }
    }

    /// Where people speak in `part_id`, one flag per [`mbl_analysis::subsync::FRAME_MS`]. Made
    /// with the subtitle extraction (one read of the file) and cached.
    pub async fn speech_map(&self, part_id: PartId) -> Result<Vec<bool>> {
        let file = speech_map_path(&self.subtitle_cache_dir(), part_id);
        if let Some(map) =
            tokio::fs::read(&file).await.ok().and_then(|b| mbl_analysis::subsync::unpack_speech(&b))
        {
            return Ok(map);
        }
        self.prepare_subtitles(part_id, true).await?;
        tokio::fs::read(&file)
            .await
            .ok()
            .and_then(|b| mbl_analysis::subsync::unpack_speech(&b))
            .ok_or_else(|| {
                Error::Unavailable("this video has no audio to match subtitles to".into())
            })
    }

    /// The WebVTT file `vtt` moved to where people speak (see [`mbl_analysis::subsync`]),
    /// cached next to it. When the match is not clearly better than the original timing, the
    /// original is kept, so a well-timed subtitle never gets worse.
    async fn synced_vtt(&self, part_id: PartId, stream_index: u32, vtt: &Path) -> Result<PathBuf> {
        let synced = vtt.with_file_name(format!("{}_{}.synced.vtt", part_id.0, stream_index));
        if tokio::fs::metadata(&synced).await.is_ok_and(|m| m.len() > 0) {
            return Ok(synced);
        }
        let speech = self.speech_map(part_id).await?;
        let text = tokio::fs::read_to_string(vtt).await.map_err(Error::internal)?;
        let cues = mbl_stream::subtitles::vtt_cues(&text);
        let found = mbl_analysis::subsync::align_to_speech(&speech, &cues, 30_000);
        let output = match found {
            Some(alignment) if alignment.is_improvement() => {
                tracing::info!(
                    part = part_id.0,
                    stream = stream_index,
                    offset_ms = alignment.offset_ms,
                    scale = alignment.scale,
                    score = format!("{:.2}→{:.2}", alignment.unshifted_score, alignment.score),
                    "subtitles matched to the speech"
                );
                mbl_stream::subtitles::shift_vtt(&text, |ms| alignment.apply(ms))
            }
            other => {
                tracing::info!(
                    part = part_id.0,
                    stream = stream_index,
                    score = other.map(|a| a.unshifted_score),
                    "subtitles already match the speech"
                );
                text
            }
        };
        let tmp = synced.with_extension("vtt.tmp");
        tokio::fs::write(&tmp, output).await.map_err(Error::internal)?;
        tokio::fs::rename(&tmp, &synced).await.map_err(Error::internal)?;
        Ok(synced)
    }
}

fn speech_map_path(cache_dir: &Path, part_id: PartId) -> PathBuf {
    cache_dir.join("subtitles").join(format!("{}.speech", part_id.0))
}

/// One JPEG frame at `position_ms`, at most `max_width` pixels wide. Seeks before opening the
/// input, so it lands on the nearest keyframe and reads little of the file.
pub(crate) async fn grab_jpeg(
    ffmpeg: &Path,
    input: &Path,
    position_ms: i64,
    max_width: u32,
) -> Result<Vec<u8>> {
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    let seconds = format!("{:.3}", position_ms.max(0) as f64 / 1000.0);
    let scale = format!("scale='min({max_width},iw)':-2");
    let mut command = tokio::process::Command::new(ffmpeg);
    command
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-ss", &seconds, "-i"])
        .arg(input)
        .args(["-map", "0:v:0", "-frames:v", "1", "-vf", &scale, "-q:v", "5"])
        .args(["-f", "image2pipe", "-c:v", "mjpeg", "pipe:1"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(TIMEOUT, command.output())
        .await
        .map_err(|_| Error::Unavailable("taking the screenshot took too long".into()))?
        .map_err(Error::internal)?;
    if !output.status.success() || output.stdout.len() < 100 || output.stdout.len() > MAX_BYTES {
        return Err(Error::Unavailable("could not take a screenshot of this video".into()));
    }
    Ok(output.stdout)
}

/// Width of a "Continue watching" snapshot.
const SNAPSHOT_WIDTH: u32 = 480;
/// Snapshots are taken at whole multiples of this, so a few seconds more watching reuses one.
const SNAPSHOT_STEP_MS: i64 = 10_000;
/// Snapshots kept on disk; the oldest go first.
const SNAPSHOTS_KEPT: usize = 400;

impl StreamService {
    /// A picture of the video at `at_ms`, for the "Continue watching" cards: where the viewer
    /// stopped rather than the poster. Rounded down to 10 seconds and cached on disk, so a card
    /// costs one FFmpeg seek the first time. Same access checks as playing the item.
    pub async fn snapshot(&self, actor: &Actor, item_id: ItemId, at_ms: i64) -> Result<Vec<u8>> {
        let first = media::parts_for_item(&self.db, item_id)
            .await?
            .into_iter()
            .find(|p| p.missing_since.is_none())
            .ok_or(Error::NotFound)?;
        let part = self.resolve_part(actor, first.id).await?;
        let mut at = at_ms.max(0) / SNAPSHOT_STEP_MS * SNAPSHOT_STEP_MS;
        if let Some(duration) = part.duration_ms.filter(|d| *d > 2_000) {
            at = at.min(duration - 2_000);
        }
        let dir = self
            .cache_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("mbl-cache"))
            .join("snapshots");
        let file = dir.join(format!("{}_{}.jpg", part.id.0, at));
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return Ok(bytes);
        }
        // Two at a time: a home page full of cards must not start a crowd of FFmpegs on a Pi.
        static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
        let _slot = SLOTS.acquire().await.map_err(Error::internal)?;
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return Ok(bytes);
        }
        let input = self.media_input(&part.path).await?;
        let jpeg = grab_jpeg(&self.ffmpeg_bin, &input, at, SNAPSHOT_WIDTH).await?;
        if tokio::fs::create_dir_all(&dir).await.is_ok() {
            let tmp = dir.join(format!(".{}_{}.tmp", part.id.0, at));
            if tokio::fs::write(&tmp, &jpeg).await.is_ok() {
                let _ = tokio::fs::rename(&tmp, &file).await;
            }
            prune_oldest(&dir, SNAPSHOTS_KEPT).await;
        }
        Ok(jpeg)
    }
}

/// Remove the oldest files in `dir` beyond `keep`.
async fn prune_oldest(dir: &Path, keep: usize) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else { return };
    let mut files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(meta) = entry.metadata().await
            && meta.is_file()
        {
            files.push((meta.modified().ok(), entry.path()));
        }
    }
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    for (_, path) in files.iter().take(files.len() - keep) {
        let _ = tokio::fs::remove_file(path).await;
    }
}

/// Ensure `raw_path` contains no parent-directory traversal (`..`) and resolves inside one of `roots`.
pub fn verify_under_library_roots(raw_path: &str, roots: &[String]) -> Result<PathBuf> {
    // Storage-node files are addressed by URI; the node itself confines them to its folder.
    if raw_path.starts_with(mbl_core::NODE_URI_SCHEME) {
        let (node, library, _) = mbl_core::parse_node_path(raw_path).ok_or(Error::Forbidden)?;
        let root = mbl_core::node_library_root(node, library);
        if roots.contains(&root) {
            return Ok(PathBuf::from(raw_path));
        }
        tracing::warn!(path = raw_path, "rejected storage-node path outside library roots");
        return Err(Error::Forbidden);
    }
    let raw = Path::new(raw_path);
    if !raw.is_absolute() || raw.components().any(|c| matches!(c, Component::ParentDir)) {
        tracing::warn!(path = raw_path, "rejected relative or parent-traversing media path");
        return Err(Error::Forbidden);
    }

    let canonical_roots: Vec<PathBuf> =
        roots.iter().filter_map(|r| dunce::canonicalize(r).ok()).collect();

    let lexically_under_root = roots.iter().any(|r| path_starts_with(raw, Path::new(r)))
        || canonical_roots.iter().any(|r| path_starts_with(raw, r));

    let canonical_file = match dunce::canonicalize(raw) {
        Ok(p) => p,
        Err(_) if !lexically_under_root => {
            tracing::warn!(path = raw_path, "rejected media path outside library roots");
            return Err(Error::Forbidden);
        }
        Err(_) => return Err(Error::NotFound),
    };

    let under_canonical_root =
        canonical_roots.iter().any(|root| path_starts_with(&canonical_file, root));
    if !under_canonical_root {
        tracing::warn!(
            path = raw_path,
            canonical = %canonical_file.display(),
            "rejected media path escaping library roots"
        );
        return Err(Error::Forbidden);
    }

    Ok(canonical_file)
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    if cfg!(windows) {
        let p = path.to_string_lossy().to_lowercase();
        let r = root.to_string_lossy().to_lowercase();
        Path::new(&p).starts_with(Path::new(&r))
    } else {
        path.starts_with(root)
    }
}

/// Whether a part path points at a storage node.
pub fn is_node_path(path: &str) -> bool {
    path.starts_with(mbl_core::NODE_URI_SCHEME)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn traversal_guard_accepts_in_root_and_rejects_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("movies");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let valid_file = root.join("film.mp4");
        std::fs::write(&valid_file, b"video").unwrap();
        let secret_file = outside.join("secret.mp4");
        std::fs::write(&secret_file, b"secret").unwrap();

        let roots = vec![root.to_str().unwrap().to_owned()];

        // Valid in-root file succeeds
        let resolved = verify_under_library_roots(valid_file.to_str().unwrap(), &roots).unwrap();
        assert_eq!(resolved, dunce::canonicalize(&valid_file).unwrap());

        // File in another directory is Forbidden
        let err = verify_under_library_roots(secret_file.to_str().unwrap(), &roots).unwrap_err();
        assert!(matches!(err, Error::Forbidden));

        // Path with `..` is Forbidden even if target exists
        let dotdot = format!("{}/../outside/secret.mp4", root.display());
        let err = verify_under_library_roots(&dotdot, &roots).unwrap_err();
        assert!(matches!(err, Error::Forbidden));

        // Missing file inside root is NotFound
        let missing = root.join("missing.mp4");
        let err = verify_under_library_roots(missing.to_str().unwrap(), &roots).unwrap_err();
        assert!(matches!(err, Error::NotFound));

        // Symlink inside root pointing outside is Forbidden
        #[cfg(unix)]
        {
            let link = root.join("escape.mp4");
            std::os::unix::fs::symlink(&secret_file, &link).unwrap();
            let err = verify_under_library_roots(link.to_str().unwrap(), &roots).unwrap_err();
            assert!(matches!(err, Error::Forbidden));
        }
    }
}
