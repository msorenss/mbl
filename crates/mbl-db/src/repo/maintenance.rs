//! Database upkeep: statistics, optimize/checkpoint/vacuum, backups, old jobs and media items
//! whose files are gone.

use mbl_core::{LibraryId, Result};
use sqlx::Row;

use crate::Db;

/// Path of the main database file, if it is a file.
pub async fn db_file(db: &Db) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
        .fetch_optional(&db.read)
        .await?
        .filter(|f: &String| !f.is_empty()))
}

/// Database size and unused space, in bytes.
pub async fn size(db: &Db) -> Result<(i64, i64)> {
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count").fetch_one(&db.write).await?;
    let free: i64 = sqlx::query_scalar("PRAGMA freelist_count").fetch_one(&db.write).await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size").fetch_one(&db.write).await?;
    Ok((pages * page_size, free * page_size))
}

/// `PRAGMA optimize`, then fold the write-ahead log back into the database file.
pub async fn optimize(db: &Db) -> Result<()> {
    sqlx::query("PRAGMA optimize").execute(&db.write).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&db.write).await?;
    Ok(())
}

/// Rebuild the file without unused pages.
pub async fn vacuum(db: &Db) -> Result<()> {
    sqlx::query("VACUUM").execute(&db.write).await?;
    Ok(())
}

/// Copy the database to `target` (a consistent snapshot).
pub async fn backup_into(db: &Db, target: &str) -> Result<()> {
    sqlx::query("VACUUM INTO ?").bind(target).execute(&db.write).await?;
    crate::restrict_to_owner(std::path::Path::new(target));
    Ok(())
}

/// Finished or failed jobs last changed before `before` (count, or delete when `apply`).
pub async fn old_jobs(db: &Db, before: i64, apply: bool) -> Result<i64> {
    if apply {
        let done = sqlx::query(
            "DELETE FROM jobs WHERE status IN ('done', 'failed', 'cancelled') AND updated_at < ?",
        )
        .bind(before)
        .execute(&db.write)
        .await?;
        return Ok(done.rows_affected() as i64);
    }
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE status IN ('done', 'failed', 'cancelled') AND updated_at < ?",
    )
    .bind(before)
    .fetch_one(&db.read)
    .await?)
}

/// A media item whose files are gone.
#[derive(Debug, Clone, PartialEq)]
pub struct GoneItem {
    pub id: i64,
    pub title: String,
    pub library_id: LibraryId,
}

/// Items in `libraries` that have files, all of them missing since before `before`, none of them
/// on a storage node.
pub async fn gone_items(db: &Db, libraries: &[LibraryId], before: i64) -> Result<Vec<GoneItem>> {
    if libraries.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "SELECT i.id, i.title, i.library_id FROM media_items i \
         WHERE EXISTS (SELECT 1 FROM media_versions v JOIN media_parts p ON p.version_id = v.id \
                       WHERE v.item_id = i.id) \
           AND NOT EXISTS (SELECT 1 FROM media_versions v JOIN media_parts p ON p.version_id = v.id \
                       WHERE v.item_id = i.id AND (p.missing_since IS NULL OR p.missing_since >= ",
    );
    qb.push_bind(before);
    qb.push(" OR p.path LIKE 'mblnode://%')) AND i.library_id IN (");
    let mut list = qb.separated(", ");
    for lib in libraries {
        list.push_bind(lib.0);
    }
    qb.push(") ORDER BY i.id");
    let rows = qb.build().fetch_all(&db.read).await?;
    Ok(rows
        .into_iter()
        .map(|r| GoneItem {
            id: r.get("id"),
            title: r.get("title"),
            library_id: LibraryId(r.get("library_id")),
        })
        .collect())
}

/// Seasons, shows, albums and artists in `libraries` with nothing left in them (and no files of
/// their own).
pub async fn empty_containers(db: &Db, libraries: &[LibraryId]) -> Result<Vec<GoneItem>> {
    if libraries.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "SELECT i.id, i.title, i.library_id FROM media_items i \
         WHERE i.kind IN ('show', 'season', 'artist', 'album') \
           AND NOT EXISTS (SELECT 1 FROM media_items c WHERE c.parent_id = i.id) \
           AND NOT EXISTS (SELECT 1 FROM media_versions v WHERE v.item_id = i.id) \
           AND i.library_id IN (",
    );
    let mut list = qb.separated(", ");
    for lib in libraries {
        list.push_bind(lib.0);
    }
    qb.push(") ORDER BY i.id");
    let rows = qb.build().fetch_all(&db.read).await?;
    Ok(rows
        .into_iter()
        .map(|r| GoneItem {
            id: r.get("id"),
            title: r.get("title"),
            library_id: LibraryId(r.get("library_id")),
        })
        .collect())
}

/// Remove items (and everything that belongs to them).
pub async fn delete_items(db: &Db, ids: &[i64]) -> Result<u64> {
    let mut removed = 0;
    for chunk in ids.chunks(200) {
        let mut qb =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("DELETE FROM media_items WHERE id IN (");
        let mut list = qb.separated(", ");
        for id in chunk {
            list.push_bind(*id);
        }
        qb.push(")");
        removed += qb.build().execute(&db.write).await?.rows_affected();
    }
    Ok(removed)
}
