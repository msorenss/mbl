//! Offline download tickets (`downloads`).

use mbl_core::{ItemId, PartId, Result, UserId};

use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DownloadRow {
    pub id: i64,
    pub user_id: UserId,
    pub item_id: ItemId,
    pub part_id: PartId,
    pub quality: String,
    pub status: String,
    pub file_path: Option<String>,
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Insert a ticket, or return the existing one for `(user, part, quality)`.
/// Returns `(row, created)`.
#[allow(clippy::too_many_arguments)]
pub async fn insert_or_get(
    db: &Db,
    user_id: UserId,
    item_id: ItemId,
    part_id: PartId,
    quality: &str,
    status: &str,
    file_path: Option<&str>,
    size_bytes: Option<i64>,
    now: i64,
) -> Result<(DownloadRow, bool)> {
    let inserted = sqlx::query(
        "INSERT INTO downloads (user_id, item_id, part_id, quality, status, file_path, size_bytes,
                                created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, part_id, quality) DO NOTHING",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(part_id)
    .bind(quality)
    .bind(status)
    .bind(file_path)
    .bind(size_bytes)
    .bind(now)
    .bind(now)
    .execute(&db.write)
    .await?
    .rows_affected()
        > 0;
    let row = sqlx::query_as::<_, DownloadRow>(
        "SELECT id, user_id, item_id, part_id, quality, status, file_path, size_bytes, error,
                created_at, updated_at
         FROM downloads WHERE user_id = ? AND part_id = ? AND quality = ?",
    )
    .bind(user_id)
    .bind(part_id)
    .bind(quality)
    .fetch_one(&db.write)
    .await?;
    Ok((row, inserted))
}

pub async fn get(db: &Db, id: i64) -> Result<Option<DownloadRow>> {
    Ok(sqlx::query_as::<_, DownloadRow>(
        "SELECT id, user_id, item_id, part_id, quality, status, file_path, size_bytes, error,
                created_at, updated_at
         FROM downloads WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&db.read)
    .await?)
}

pub async fn list_for_user(db: &Db, user_id: UserId) -> Result<Vec<DownloadRow>> {
    Ok(sqlx::query_as::<_, DownloadRow>(
        "SELECT id, user_id, item_id, part_id, quality, status, file_path, size_bytes, error,
                created_at, updated_at
         FROM downloads WHERE user_id = ? ORDER BY created_at DESC, id DESC LIMIT 1000",
    )
    .bind(user_id)
    .fetch_all(&db.read)
    .await?)
}

/// A finished transcode of the same part and quality (from any user), for reuse.
pub async fn find_ready_file(
    db: &Db,
    part_id: PartId,
    quality: &str,
) -> Result<Option<(String, i64)>> {
    let row: Option<(Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT file_path, size_bytes FROM downloads
         WHERE part_id = ? AND quality = ? AND status = 'ready' AND file_path IS NOT NULL
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(part_id)
    .bind(quality)
    .fetch_optional(&db.read)
    .await?;
    Ok(row.and_then(|(p, s)| Some((p?, s.unwrap_or(0)))))
}

/// Number of tickets that still reference `file_path`.
pub async fn count_file_refs(db: &Db, file_path: &str) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM downloads WHERE file_path = ?")
        .bind(file_path)
        .fetch_one(&db.write)
        .await?)
}

pub async fn set_status(
    db: &Db,
    id: i64,
    status: &str,
    file_path: Option<&str>,
    size_bytes: Option<i64>,
    error: Option<&str>,
    now: i64,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE downloads SET status = ?, file_path = ?, size_bytes = ?, error = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(status)
    .bind(file_path)
    .bind(size_bytes)
    .bind(error)
    .bind(now)
    .bind(id)
    .execute(&db.write)
    .await?
    .rows_affected()
        > 0)
}

/// New state for every pending/transcoding ticket of one `(part, quality)` pair.
#[derive(Debug, Clone, Copy, Default)]
pub struct DownloadOutcome<'a> {
    pub status: &'a str,
    pub file_path: Option<&'a str>,
    pub size_bytes: Option<i64>,
    pub error: Option<&'a str>,
}

/// Mark every pending/transcoding ticket for `(part, quality)` with the same outcome.
pub async fn complete_pending(
    db: &Db,
    part_id: PartId,
    quality: &str,
    outcome: DownloadOutcome<'_>,
    now: i64,
) -> Result<u64> {
    Ok(sqlx::query(
        "UPDATE downloads SET status = ?, file_path = ?, size_bytes = ?, error = ?, updated_at = ?
         WHERE part_id = ? AND quality = ? AND status IN ('pending', 'transcoding')",
    )
    .bind(outcome.status)
    .bind(outcome.file_path)
    .bind(outcome.size_bytes)
    .bind(outcome.error)
    .bind(now)
    .bind(part_id)
    .bind(quality)
    .execute(&db.write)
    .await?
    .rows_affected())
}

pub async fn delete(db: &Db, id: i64) -> Result<bool> {
    Ok(sqlx::query("DELETE FROM downloads WHERE id = ?")
        .bind(id)
        .execute(&db.write)
        .await?
        .rows_affected()
        > 0)
}
