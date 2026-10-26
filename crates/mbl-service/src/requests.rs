//! Built-in Media Requests, Sonarr/Radarr Bridge & Hardlink Storage Hygiene (M7.20).

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
    pub kind: String, // "movie" or "show"
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i32>,
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
    http_client: reqwest::Client,
}

impl MediaRequestService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db, http_client: reqwest::Client::new() }
    }

    pub async fn create_request(
        &self,
        actor: &Actor,
        req: CreateMediaRequest,
    ) -> Result<MediaRequestRow> {
        let user_id = actor.user_id;
        let id = m7::create_media_request(
            &self.db,
            user_id,
            &req.kind,
            req.tmdb_id,
            &req.title,
            req.year,
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
        if !m7::update_media_request_status(&self.db, id, status, arr_id).await? {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    /// Forward approved request to Radarr (movie) or Sonarr (series).
    pub async fn forward_to_arr(
        &self,
        actor: &Actor,
        request_id: RequestId,
        arr_url: &str,
        api_key: &str,
        root_folder_path: &str,
        quality_profile_id: i64,
    ) -> Result<i64> {
        actor.require_admin()?;
        let requests = m7::list_media_requests(&self.db, None, None).await?;
        let req = requests.into_iter().find(|r| r.id == request_id).ok_or(Error::NotFound)?;

        let endpoint = if req.kind == "movie" {
            format!("{}/api/v3/movie", arr_url.trim_end_matches('/'))
        } else {
            format!("{}/api/v3/series", arr_url.trim_end_matches('/'))
        };

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ArrPayload<'a> {
            title: &'a str,
            tmdb_id: i64,
            quality_profile_id: i64,
            root_folder_path: &'a str,
            monitored: bool,
            add_options: ArrAddOptions,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ArrAddOptions {
            search_for_missing_movie: bool,
            search_for_missing_episodes: bool,
        }
        #[derive(Deserialize)]
        struct ArrResp {
            id: i64,
        }

        let body = ArrPayload {
            title: &req.title,
            tmdb_id: req.tmdb_id,
            quality_profile_id,
            root_folder_path,
            monitored: true,
            add_options: ArrAddOptions {
                search_for_missing_movie: req.kind == "movie",
                search_for_missing_episodes: req.kind == "show",
            },
        };

        let resp = self
            .http_client
            .post(&endpoint)
            .header("X-Api-Key", api_key)
            .json(&body)
            .send()
            .await
            .map_err(Error::internal)?;

        if !resp.status().is_success() {
            return Err(Error::External(format!(
                "Radarr/Sonarr returned status {}",
                resp.status()
            )));
        }

        let res_body: ArrResp = resp.json().await.map_err(Error::internal)?;
        m7::update_media_request_status(&self.db, request_id, "approved", Some(res_body.id))
            .await?;
        Ok(res_body.id)
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
