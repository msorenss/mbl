//! Direct-play, HLS session lookup, and media part resolution with library ACL and path-traversal enforcement.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use mbl_core::{Actor, Error, ItemId, LibraryId, PartId, Result, StreamKind};
use mbl_db::Db;
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::media;
use mbl_stream::subtitles::{SubtitleFormat, SubtitleSource, extract_or_convert_subtitle};
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

    /// Resolve, convert (if needed), and cache a subtitle stream (`.vtt` or `.ass`) for `part_id`.
    pub async fn resolve_subtitle(
        &self,
        actor: &Actor,
        part_id: PartId,
        stream_index: u32,
        format: SubtitleFormat,
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
                SubtitleSource::Embedded { media_path, stream_index, codec }
            }
        };

        let cache_dir = self
            .cache_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("mbl-subtitle-cache"));
        let ffmpeg_bin = self
            .hls
            .as_ref()
            .map(|h| h.config().ffmpeg_bin.as_path())
            .unwrap_or(self.ffmpeg_bin.as_path());

        extract_or_convert_subtitle(&cache_dir, ffmpeg_bin, part_id, stream_index, &source, format)
            .await
    }
}

/// Ensure `raw_path` contains no parent-directory traversal (`..`) and resolves inside one of `roots`.
pub fn verify_under_library_roots(raw_path: &str, roots: &[String]) -> Result<PathBuf> {
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
