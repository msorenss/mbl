//! Built-in Media Requests, Sonarr/Radarr Bridge & Hardlink Storage Hygiene (M7.20).

use crate::arr::{ArrKind, ArrLink, ArrService};
use mbl_core::{Actor, Error, RequestId, Result};
use mbl_db::Db;
use mbl_db::repo::m7;
pub use mbl_db::repo::m7::MediaRequestRow;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateMediaRequest {
    /// "movie", "show", "artist" or "album".
    pub kind: String,
    /// TMDB ID for movies and shows.
    #[serde(default)]
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i32>,
    /// MusicBrainz artist ID or release group ID for artists and albums.
    #[serde(default)]
    pub musicbrainz_id: Option<String>,
    /// Artist name for albums.
    #[serde(default)]
    pub artist: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MissingEpisodeInfo {
    pub season_number: i64,
    pub missing_episode_numbers: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorageHardlinkReport {
    pub total_scanned_files: usize,
    pub total_file_size_bytes: u64,
    pub unique_inode_count: usize,
    pub hardlink_shared_files_count: usize,
    pub estimated_net_freed_bytes_if_deleted: u64,
}

#[derive(Debug, Clone)]
pub struct MediaRequestService {
    db: Arc<Db>,
    pub arr: ArrService,
}

const STATUSES: [&str; 4] = ["pending", "approved", "declined", "available"];

impl MediaRequestService {
    pub fn new(db: Arc<Db>) -> Self {
        let arr = ArrService::new((*db).clone());
        Self { db, arr }
    }

    pub async fn create_request(
        &self,
        actor: &Actor,
        req: CreateMediaRequest,
    ) -> Result<MediaRequestRow> {
        let title = req.title.trim();
        if title.is_empty() || title.chars().count() > 300 {
            return Err(Error::Validation("Enter a title of at most 300 characters".into()));
        }
        let mbid = req.musicbrainz_id.as_deref().map(str::trim).filter(|m| !m.is_empty());
        match req.kind.as_str() {
            "movie" | "show" if req.tmdb_id < 1 => {
                return Err(Error::Validation("Movies and shows need a TMDB ID".into()));
            }
            "movie" | "show" => {}
            "artist" | "album" => {
                let valid = mbid.is_some_and(|m| {
                    m.len() == 36 && m.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
                });
                if !valid {
                    return Err(Error::Validation(
                        "Artists and albums need a MusicBrainz ID (search to fill it in)".into(),
                    ));
                }
            }
            other => return Err(Error::Validation(format!("Unknown request kind '{other}'"))),
        }
        let music = matches!(req.kind.as_str(), "artist" | "album");
        let id = m7::create_media_request(
            &self.db,
            actor.user_id,
            &m7::NewMediaRequest {
                kind: &req.kind,
                tmdb_id: if music { 0 } else { req.tmdb_id },
                title,
                year: req.year,
                musicbrainz_id: if music { mbid } else { None },
                artist: req.artist.as_deref().map(str::trim).filter(|a| !a.is_empty() && music),
            },
        )
        .await?;

        let list = m7::list_media_requests(&self.db, None, None).await?;
        list.into_iter().find(|r| r.id == id).ok_or(Error::NotFound)
    }

    pub async fn list_requests(
        &self,
        actor: &Actor,
        status: Option<&str>,
    ) -> Result<Vec<MediaRequestRow>> {
        // Requests whose title has shown up in a library are fulfilled.
        m7::mark_requests_available_from_library(&self.db).await?;
        let user_filter = if actor.is_admin { None } else { Some(actor.user_id) };
        m7::list_media_requests(&self.db, user_filter, status).await
    }

    /// "Open in Radarr/Sonarr/Lidarr" for a request (admin only).
    pub async fn arr_link(&self, actor: &Actor, id: RequestId) -> Result<Option<ArrLink>> {
        actor.require_admin()?;
        let request = m7::list_media_requests(&self.db, None, None)
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        let tmdb = Some(request.tmdb_id).filter(|id| *id > 0);
        self.arr.title_link(actor, &request.kind, request.arr_id, tmdb).await
    }

    pub async fn update_status(
        &self,
        actor: &Actor,
        id: RequestId,
        status: &str,
        arr_id: Option<i64>,
    ) -> Result<()> {
        actor.require_admin()?;
        if !STATUSES.contains(&status) {
            return Err(Error::Validation(format!("Unknown status '{status}'")));
        }
        let current = m7::list_media_requests(&self.db, None, None)
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)?;
        // Approving hands the title to Radarr/Sonarr/Lidarr when that service is turned on. If
        // adding fails the request keeps its old status so the admin can retry. Approving an
        // already approved request that was never sent is a resend, which needs the service on.
        let resend = current.status == "approved" && current.arr_id.is_none();
        let arr_id = if status == "approved" && (current.status != "approved" || resend) {
            let added = self.arr.add(&current).await.inspect_err(|e| {
                let service = ArrKind::for_request(&current.kind).map_or("none", ArrKind::name);
                tracing::warn!(
                    service,
                    failure = e.code(),
                    "adding title to download service failed"
                );
            })?;
            match added {
                Some((_, added)) => Some(added),
                None if resend => {
                    tracing::info!("resend skipped: download service is turned off");
                    let service = ArrKind::for_request(&current.kind)
                        .map_or("No download service", ArrKind::name);
                    return Err(Error::Validation(format!(
                        "{service} is turned off, so nothing was sent"
                    )));
                }
                None => arr_id,
            }
        } else {
            arr_id
        };
        if !m7::update_media_request_status(&self.db, id, status, arr_id).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    /// Detect missing episode numbers in a list of existing episode indices.
    pub fn find_missing_episodes(
        season_number: i64,
        existing_episode_numbers: &[i64],
    ) -> MissingEpisodeInfo {
        if existing_episode_numbers.is_empty() {
            return MissingEpisodeInfo { season_number, missing_episode_numbers: Vec::new() };
        }

        let max_ep = *existing_episode_numbers.iter().max().unwrap_or(&0);
        let existing_set: HashSet<i64> = existing_episode_numbers.iter().copied().collect();

        let mut missing = Vec::new();
        for ep in 1..max_ep {
            if !existing_set.contains(&ep) {
                missing.push(ep);
            }
        }

        MissingEpisodeInfo { season_number, missing_episode_numbers: missing }
    }

    /// Hardlink-aware storage analysis. A file whose data also lives under another name
    /// (for example in a download folder) frees no space when only the library copy is deleted.
    pub fn analyze_hardlinks(paths: &[&Path]) -> StorageHardlinkReport {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;

        let mut total_scanned_files = 0;
        let mut total_file_size_bytes = 0u64;
        let mut hardlink_shared_files_count = 0;
        let mut freed = 0u64;
        let mut unique_inodes = HashSet::new();

        for path in paths {
            let Ok(meta) = fs::metadata(path) else { continue };
            if !meta.is_file() {
                continue;
            }
            total_scanned_files += 1;
            let size = meta.len();
            total_file_size_bytes += size;
            #[cfg(unix)]
            let (identity, shared) = ((meta.dev(), meta.ino()), meta.nlink() > 1);
            #[cfg(not(unix))]
            let (identity, shared) = ((0u64, total_scanned_files as u64), false);
            unique_inodes.insert(identity);
            if shared {
                hardlink_shared_files_count += 1;
            } else {
                freed += size;
            }
        }

        StorageHardlinkReport {
            total_scanned_files,
            total_file_size_bytes,
            unique_inode_count: unique_inodes.len(),
            hardlink_shared_files_count,
            estimated_net_freed_bytes_if_deleted: freed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_missing_episodes_in_season() {
        let existing = vec![1, 2, 4, 5, 7];
        let missing = MediaRequestService::find_missing_episodes(1, &existing);
        assert_eq!(missing.missing_episode_numbers, vec![3, 6]);
    }

    #[cfg(unix)]
    #[test]
    fn hardlinked_files_free_no_space() {
        let dir = tempfile::tempdir().unwrap();
        let single = dir.path().join("single.mkv");
        let linked = dir.path().join("linked.mkv");
        fs::write(&single, vec![0u8; 100]).unwrap();
        fs::write(&linked, vec![0u8; 40]).unwrap();
        fs::hard_link(&linked, dir.path().join("download.mkv")).unwrap();
        let report = MediaRequestService::analyze_hardlinks(&[&single, &linked]);
        assert_eq!(report.total_scanned_files, 2);
        assert_eq!(report.total_file_size_bytes, 140);
        assert_eq!(report.hardlink_shared_files_count, 1);
        assert_eq!(report.estimated_net_freed_bytes_if_deleted, 100);
    }
}
