//! Built-in Media Requests, Sonarr/Radarr Bridge & Hardlink Storage Hygiene (M7.20).

use crate::arr::{ArrKind, ArrService};
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

    /// Linux/Unix hardlink-aware storage analysis using inode and `st_nlink`.
    pub fn analyze_hardlinks(paths: &[&Path]) -> StorageHardlinkReport {
        let mut total_scanned_files = 0;
        let mut total_file_size_bytes = 0u64;
        #[cfg(unix)]
        let mut hardlink_shared_files_count = 0;
        #[cfg(not(unix))]
        let hardlink_shared_files_count = 0;
        let mut unique_inodes = HashSet::new();

        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;

        for path in paths {
            if let Ok(meta) = fs::metadata(path)
                && meta.is_file()
            {
                total_scanned_files += 1;
                let size = meta.len();
                total_file_size_bytes += size;

                #[cfg(unix)]
                {
                    let ino = meta.ino();
                    let nlink = meta.nlink();
                    unique_inodes.insert(ino);
                    if nlink > 1 {
                        hardlink_shared_files_count += 1;
                    }
                }
                #[cfg(not(unix))]
                {
                    unique_inodes.insert(total_scanned_files);
                }
            }
        }

        // If files are hardlinked to external torrent dirs (nlink > 1), deleting them in MBL
        // frees 0 bytes until the other hardlink is also removed.
        let estimated_net_freed =
            if hardlink_shared_files_count == total_scanned_files && total_scanned_files > 0 {
                0
            } else {
                total_file_size_bytes
            };

        StorageHardlinkReport {
            total_scanned_files,
            total_file_size_bytes,
            unique_inode_count: unique_inodes.len(),
            hardlink_shared_files_count,
            estimated_net_freed_bytes_if_deleted: estimated_net_freed,
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
}
