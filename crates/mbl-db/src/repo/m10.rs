//! Database operations for Milestone M10:
//! - Plex & Jellyfin watch history and state import matching and upserting.

use mbl_core::{ItemId, Result, UserId};
use sqlx::Sqlite;

use crate::Db;

/// Finds a media item by IMDb, TMDB, or TVDB ID.
pub async fn find_item_by_external_ids(
    db: &Db,
    imdb_id: Option<&str>,
    tmdb_id: Option<i64>,
    tvdb_id: Option<i64>,
) -> Result<Option<ItemId>> {
    if let Some(imdb) = imdb_id.filter(|s| !s.is_empty()) {
        let id = sqlx::query_scalar::<Sqlite, i64>(
            "SELECT id FROM media_items WHERE imdb_id = ? LIMIT 1",
        )
        .bind(imdb)
        .fetch_optional(&db.read)
        .await?;
        if let Some(id) = id {
            return Ok(Some(ItemId(id)));
        }
    }

    if let Some(tmdb) = tmdb_id.filter(|&id| id > 0) {
        let id = sqlx::query_scalar::<Sqlite, i64>(
            "SELECT id FROM media_items WHERE tmdb_id = ? LIMIT 1",
        )
        .bind(tmdb)
        .fetch_optional(&db.read)
        .await?;
        if let Some(id) = id {
            return Ok(Some(ItemId(id)));
        }
    }

    if let Some(tvdb) = tvdb_id.filter(|&id| id > 0) {
        let id = sqlx::query_scalar::<Sqlite, i64>(
            "SELECT id FROM media_items WHERE tvdb_id = ? LIMIT 1",
        )
        .bind(tvdb)
        .fetch_optional(&db.read)
        .await?;
        if let Some(id) = id {
            return Ok(Some(ItemId(id)));
        }
    }

    Ok(None)
}

/// Fallback: finds an item by title and year.
pub async fn find_item_by_title_and_year(
    db: &Db,
    title: &str,
    year: Option<i32>,
) -> Result<Option<ItemId>> {
    let clean_title = title.trim();
    if clean_title.is_empty() {
        return Ok(None);
    }

    let id = if let Some(y) = year {
        sqlx::query_scalar::<Sqlite, i64>(
            "SELECT id FROM media_items WHERE LOWER(title) = LOWER(?) AND year = ? LIMIT 1",
        )
        .bind(clean_title)
        .bind(y)
        .fetch_optional(&db.read)
        .await?
    } else {
        sqlx::query_scalar::<Sqlite, i64>(
            "SELECT id FROM media_items WHERE LOWER(title) = LOWER(?) LIMIT 1",
        )
        .bind(clean_title)
        .fetch_optional(&db.read)
        .await?
    };

    Ok(id.map(ItemId))
}

/// Upserts imported watch state for a user.
pub async fn upsert_watch_state_imported(
    db: &Db,
    user_id: UserId,
    item_id: ItemId,
    view_offset_ms: i64,
    view_count: i64,
    last_viewed_at: Option<i64>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO watch_state (user_id, item_id, view_offset_ms, view_count, last_viewed_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET
           view_offset_ms = excluded.view_offset_ms,
           view_count = MAX(watch_state.view_count, excluded.view_count),
           last_viewed_at = COALESCE(excluded.last_viewed_at, watch_state.last_viewed_at)",
    )
    .bind(user_id.0)
    .bind(item_id.0)
    .bind(view_offset_ms)
    .bind(view_count)
    .bind(last_viewed_at)
    .execute(&db.write)
    .await?;

    Ok(())
}
