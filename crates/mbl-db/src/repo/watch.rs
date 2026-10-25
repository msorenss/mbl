//! Per-user watch state (`watch_state`) and playback history (`watch_history`).

use mbl_core::{ItemId, LibraryId, MediaKind, Result, UserId};
use serde::Serialize;
use utoipa::ToSchema;

use crate::Db;
use crate::repo::items::WatchState;

/// A recorded playback session in `watch_history`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WatchHistoryEntry {
    pub id: i64,
    pub user_id: UserId,
    pub item_id: ItemId,
    pub started_at: i64,
    pub stopped_at: Option<i64>,
    pub method: Option<String>,
    pub client: Option<String>,
    pub network: Option<String>,
}

/// Parameters for inserting a `watch_history` row on playback stop.
#[derive(Debug, Clone)]
pub struct NewWatchHistory<'a> {
    pub user_id: UserId,
    pub item_id: ItemId,
    pub started_at: i64,
    pub stopped_at: i64,
    pub method: &'a str,
    pub client: Option<&'a str>,
    pub network: &'a str,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WatchStateRow {
    view_offset_ms: i64,
    view_count: i64,
    last_viewed_at: Option<i64>,
}

/// Fetch the current watch state for `(user_id, item_id)`.
pub async fn get(db: &Db, user_id: UserId, item_id: ItemId) -> Result<WatchState> {
    let row = sqlx::query_as::<_, WatchStateRow>(
        "SELECT view_offset_ms, view_count, last_viewed_at
         FROM watch_state WHERE user_id = ? AND item_id = ?",
    )
    .bind(user_id)
    .bind(item_id)
    .fetch_optional(&db.read)
    .await?;

    Ok(match row {
        Some(r) => WatchState {
            view_offset_ms: r.view_offset_ms,
            view_count: r.view_count,
            last_viewed_at: r.last_viewed_at,
        },
        None => WatchState::default(),
    })
}

/// Store a resume offset (`view_offset_ms`) while preserving `view_count`.
pub async fn save_progress_offset(
    db: &Db,
    user_id: UserId,
    item_id: ItemId,
    offset_ms: i64,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO watch_state (user_id, item_id, view_offset_ms, view_count, last_viewed_at)
         VALUES (?, ?, ?, 0, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET
           view_offset_ms = excluded.view_offset_ms,
           last_viewed_at = excluded.last_viewed_at",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(offset_ms)
    .bind(now)
    .execute(&db.write)
    .await?;
    Ok(())
}

/// Mark a single item watched during playback completion, resetting `view_offset_ms = 0` and
/// optionally incrementing `view_count`.
pub async fn complete_playback(
    db: &Db,
    user_id: UserId,
    item_id: ItemId,
    increment_count: bool,
    now: i64,
) -> Result<()> {
    if increment_count {
        sqlx::query(
            "INSERT INTO watch_state (user_id, item_id, view_offset_ms, view_count, last_viewed_at)
             VALUES (?, ?, 0, 1, ?)
             ON CONFLICT(user_id, item_id) DO UPDATE SET
               view_offset_ms = 0,
               view_count = watch_state.view_count + 1,
               last_viewed_at = excluded.last_viewed_at",
        )
        .bind(user_id)
        .bind(item_id)
        .bind(now)
        .execute(&db.write)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO watch_state (user_id, item_id, view_offset_ms, view_count, last_viewed_at)
             VALUES (?, ?, 0, 1, ?)
             ON CONFLICT(user_id, item_id) DO UPDATE SET
               view_offset_ms = 0,
               view_count = MAX(watch_state.view_count, 1),
               last_viewed_at = excluded.last_viewed_at",
        )
        .bind(user_id)
        .bind(item_id)
        .bind(now)
        .execute(&db.write)
        .await?;
    }
    Ok(())
}

/// Mark a list of items watched (`view_offset_ms = 0`, `view_count = MAX(view_count, 1)`).
pub async fn set_watched_batch(
    db: &Db,
    user_id: UserId,
    item_ids: &[ItemId],
    now: i64,
) -> Result<()> {
    let mut tx = db.write.begin().await?;
    for &item_id in item_ids {
        sqlx::query(
            "INSERT INTO watch_state (user_id, item_id, view_offset_ms, view_count, last_viewed_at)
             VALUES (?, ?, 0, 1, ?)
             ON CONFLICT(user_id, item_id) DO UPDATE SET
               view_offset_ms = 0,
               view_count = MAX(watch_state.view_count, 1),
               last_viewed_at = excluded.last_viewed_at",
        )
        .bind(user_id)
        .bind(item_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Mark a list of items unwatched (`view_offset_ms = 0`, `view_count = 0`, `last_viewed_at = NULL`).
pub async fn set_unwatched_batch(db: &Db, user_id: UserId, item_ids: &[ItemId]) -> Result<()> {
    let mut tx = db.write.begin().await?;
    for &item_id in item_ids {
        sqlx::query("DELETE FROM watch_state WHERE user_id = ? AND item_id = ?")
            .bind(user_id)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Resolve `root_id` and all descendant items that should cascade when marking watched/unwatched
/// (e.g. show -> seasons + episodes; season -> episodes; artist -> albums + tracks; album -> tracks).
pub async fn cascade_item_ids(
    db: &Db,
    root_id: ItemId,
) -> Result<Option<(LibraryId, Vec<ItemId>)>> {
    let Some((library_id, kind)): Option<(LibraryId, MediaKind)> =
        sqlx::query_as("SELECT library_id, kind FROM media_items WHERE id = ?")
            .bind(root_id)
            .fetch_optional(&db.read)
            .await?
    else {
        return Ok(None);
    };

    let mut ids = vec![root_id];
    match kind {
        MediaKind::Show | MediaKind::Artist => {
            let children: Vec<ItemId> = sqlx::query_scalar(
                "SELECT id FROM media_items WHERE parent_id = ? ORDER BY idx, id",
            )
            .bind(root_id)
            .fetch_all(&db.read)
            .await?;
            for &child_id in &children {
                ids.push(child_id);
                let grandchildren: Vec<ItemId> = sqlx::query_scalar(
                    "SELECT id FROM media_items WHERE parent_id = ? ORDER BY idx, id",
                )
                .bind(child_id)
                .fetch_all(&db.read)
                .await?;
                ids.extend(grandchildren);
            }
        }
        MediaKind::Season | MediaKind::Album => {
            let children: Vec<ItemId> = sqlx::query_scalar(
                "SELECT id FROM media_items WHERE parent_id = ? ORDER BY idx, id",
            )
            .bind(root_id)
            .fetch_all(&db.read)
            .await?;
            ids.extend(children);
        }
        _ => {}
    }

    Ok(Some((library_id, ids)))
}

/// Insert a `watch_history` entry when a playback session stops.
pub async fn insert_history(db: &Db, h: &NewWatchHistory<'_>) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO watch_history (user_id, item_id, started_at, stopped_at, method, client, network)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(h.user_id)
    .bind(h.item_id)
    .bind(h.started_at)
    .bind(h.stopped_at)
    .bind(h.method)
    .bind(h.client)
    .bind(h.network)
    .fetch_one(&db.write)
    .await?;
    Ok(id)
}

/// List recent `watch_history` entries for `user_id`, newest first.
pub async fn list_history(db: &Db, user_id: UserId, limit: u32) -> Result<Vec<WatchHistoryEntry>> {
    Ok(sqlx::query_as::<_, WatchHistoryEntry>(
        "SELECT id, user_id, item_id, started_at, stopped_at, method, client, network
         FROM watch_history WHERE user_id = ? ORDER BY started_at DESC, id DESC LIMIT ?",
    )
    .bind(user_id)
    .bind(i64::from(limit))
    .fetch_all(&db.read)
    .await?)
}
