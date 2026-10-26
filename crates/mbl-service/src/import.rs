//! Watch history and library metadata import service from Plex and Jellyfin (M10.5).

use std::collections::HashMap;
use std::sync::Arc;

use mbl_core::{Actor, Result};
use mbl_db::Db;
use mbl_db::repo::{m10, watch};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// An item exported from Plex or retrieved via Plex token.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlexImportItem {
    pub title: String,
    pub year: Option<i32>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    pub view_count: Option<i64>,
    pub view_offset_ms: Option<i64>,
    pub last_viewed_at: Option<i64>,
}

/// Request payload containing Plex history items.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlexImportRequest {
    pub items: Vec<PlexImportItem>,
}

/// An item exported from Jellyfin or retrieved via Jellyfin API.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JellyfinImportItem {
    pub name: String,
    pub production_year: Option<i32>,
    pub provider_ids: Option<HashMap<String, String>>,
    pub played: Option<bool>,
    pub play_count: Option<i64>,
    pub playback_position_ticks: Option<i64>,
    pub last_played_date: Option<String>,
}

/// Request payload containing Jellyfin items.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JellyfinImportRequest {
    pub items: Vec<JellyfinImportItem>,
}

/// Summary of watch history import results.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub total_items: u32,
    pub matched_items: u32,
    pub updated_watch_state: u32,
    pub unmatched: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ImportService {
    db: Arc<Db>,
}

impl ImportService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Imports Plex watch state and history into the user's account.
    pub async fn import_plex(
        &self,
        actor: &Actor,
        req: PlexImportRequest,
    ) -> Result<ImportSummary> {
        let total_items = req.items.len() as u32;
        let mut matched_items = 0u32;
        let mut updated_watch_state = 0u32;
        let mut unmatched = Vec::new();

        let now = mbl_core::time::now_ms();

        for item in req.items {
            let matched_id = m10::find_item_by_external_ids(
                &self.db,
                item.imdb_id.as_deref(),
                item.tmdb_id,
                item.tvdb_id,
            )
            .await?;

            let final_id = match matched_id {
                Some(id) => Some(id),
                None => m10::find_item_by_title_and_year(&self.db, &item.title, item.year).await?,
            };

            if let Some(item_id) = final_id {
                matched_items += 1;
                let view_count = item.view_count.unwrap_or(1);
                let view_offset = item.view_offset_ms.unwrap_or(0);
                let last_viewed = item.last_viewed_at.unwrap_or(now);

                m10::upsert_watch_state_imported(
                    &self.db,
                    actor.user_id,
                    item_id,
                    view_offset,
                    view_count,
                    Some(last_viewed),
                )
                .await?;

                let _ = watch::insert_history(
                    &self.db,
                    &watch::NewWatchHistory {
                        user_id: actor.user_id,
                        item_id,
                        started_at: last_viewed.saturating_sub(7200 * 1000),
                        stopped_at: last_viewed,
                        method: "import",
                        client: Some("plex"),
                        network: "lan",
                    },
                )
                .await;

                updated_watch_state += 1;
            } else {
                unmatched.push(format!("{} ({:?})", item.title, item.year));
            }
        }

        Ok(ImportSummary { total_items, matched_items, updated_watch_state, unmatched })
    }

    /// Imports Jellyfin watch state and history into the user's account.
    pub async fn import_jellyfin(
        &self,
        actor: &Actor,
        req: JellyfinImportRequest,
    ) -> Result<ImportSummary> {
        let total_items = req.items.len() as u32;
        let mut matched_items = 0u32;
        let mut updated_watch_state = 0u32;
        let mut unmatched = Vec::new();

        let now = mbl_core::time::now_ms();

        for item in req.items {
            let providers = item.provider_ids.unwrap_or_default();
            let imdb = providers.get("Imdb").or_else(|| providers.get("imdb")).map(|s| s.as_str());
            let tmdb = providers
                .get("Tmdb")
                .or_else(|| providers.get("tmdb"))
                .and_then(|s| s.parse::<i64>().ok());
            let tvdb = providers
                .get("Tvdb")
                .or_else(|| providers.get("tvdb"))
                .and_then(|s| s.parse::<i64>().ok());

            let matched_id = m10::find_item_by_external_ids(&self.db, imdb, tmdb, tvdb).await?;

            let final_id = match matched_id {
                Some(id) => Some(id),
                None => {
                    m10::find_item_by_title_and_year(&self.db, &item.name, item.production_year)
                        .await?
                }
            };

            if let Some(item_id) = final_id {
                matched_items += 1;
                let view_count = item
                    .play_count
                    .unwrap_or_else(|| if item.played.unwrap_or(false) { 1 } else { 0 });
                // 1 tick = 100 ns = 0.0001 ms -> ms = ticks / 10_000
                let view_offset =
                    item.playback_position_ticks.map(|ticks| ticks / 10_000).unwrap_or(0);

                let last_viewed = now;

                m10::upsert_watch_state_imported(
                    &self.db,
                    actor.user_id,
                    item_id,
                    view_offset,
                    view_count,
                    Some(last_viewed),
                )
                .await?;

                let _ = watch::insert_history(
                    &self.db,
                    &watch::NewWatchHistory {
                        user_id: actor.user_id,
                        item_id,
                        started_at: last_viewed.saturating_sub(7200 * 1000),
                        stopped_at: last_viewed,
                        method: "import",
                        client: Some("jellyfin"),
                        network: "lan",
                    },
                )
                .await;

                updated_watch_state += 1;
            } else {
                unmatched.push(format!("{} ({:?})", item.name, item.production_year));
            }
        }

        Ok(ImportSummary { total_items, matched_items, updated_watch_state, unmatched })
    }
}
