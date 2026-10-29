//! Atomic publication of finalized recordings into the ordinary playback catalogue.
use crate::{
    Db,
    repo::media::{self, NewItem, NewPart, NewStream, VersionSummary},
};
use mbl_core::{Error, ItemId, LibraryId, MediaKind, RecordingId, Result};

pub struct RecordedMedia<'a> {
    pub recovered: bool,
    pub incomplete_reason: Option<&'a str>,
    pub root: &'a str,
    pub path: &'a str,
    pub size: i64,
    pub mtime: i64,
    pub duration_ms: i64,
    pub probe: &'a str,
    pub chapters: &'a str,
    pub version: VersionSummary<'a>,
    pub streams: Vec<NewStream<'a>>,
}

pub async fn is_library(db: &Db, library: LibraryId) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM dvr_libraries WHERE library_id=?)")
        .bind(library.0)
        .fetch_one(&db.read)
        .await?)
}

pub async fn library_id(db: &Db, root: &str) -> Result<Option<LibraryId>> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT library_id FROM dvr_libraries WHERE path=?")
        .bind(root)
        .fetch_optional(&db.read)
        .await?
        .map(LibraryId))
}

pub async fn ensure_library(db: &Db, root: &str, now: i64) -> Result<LibraryId> {
    let mut tx = db.write.begin().await?;
    let library = ensure_library_on(&mut tx, root, now).await?;
    tx.commit().await?;
    Ok(library)
}

async fn ensure_library_on(
    conn: &mut sqlx::SqliteConnection,
    root: &str,
    now: i64,
) -> Result<LibraryId> {
    let library: Option<i64> =
        sqlx::query_scalar("SELECT library_id FROM dvr_libraries WHERE path=?")
            .bind(root)
            .fetch_optional(&mut *conn)
            .await?;
    let library = match library {
        Some(id) => LibraryId(id),
        None => {
            let id: i64 = sqlx::query_scalar("INSERT INTO libraries (name,kind,created_at) VALUES ('TV recordings','homevideo',?) RETURNING id")
                .bind(now).fetch_one(&mut *conn).await?;
            sqlx::query("INSERT INTO library_paths (library_id,path) VALUES (?,?)")
                .bind(id)
                .bind(root)
                .execute(&mut *conn)
                .await?;
            sqlx::query("INSERT INTO dvr_libraries (library_id,path) VALUES (?,?)")
                .bind(id)
                .bind(root)
                .execute(&mut *conn)
                .await?;
            LibraryId(id)
        }
    };
    Ok(library)
}

pub async fn publish(
    db: &Db,
    id: RecordingId,
    media: &RecordedMedia<'_>,
    now: i64,
) -> Result<ItemId> {
    let mut tx = db.write.begin().await?;
    // This transaction also serializes cancellation and competing publication attempts.
    let row: Option<(String, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT title, item_id, summary FROM recordings WHERE id=? AND deleted_at IS NULL AND (status IN ('recording','completed','partial') OR (? AND status='failed'))",
    )
    .bind(id.0)
    .bind(media.recovered && media.incomplete_reason.is_some())
    .fetch_optional(&mut *tx)
    .await?;
    let (title, existing, summary) = row.ok_or(Error::NotFound)?;
    if let Some(item) = existing {
        return Ok(ItemId(item));
    }
    let library = ensure_library_on(&mut tx, media.root, now).await?;
    let assigned: Option<i64> = sqlx::query_scalar("SELECT library_id FROM recordings WHERE id=?")
        .bind(id.0)
        .fetch_one(&mut *tx)
        .await?;
    if assigned.is_some_and(|id| id != library.0) {
        return Err(Error::Conflict("DVR library changed during recording".into()));
    }
    // The ordinary library/player must not present a shortened file as the full programme.
    let (title, summary) = if let Some(reason) = media.incomplete_reason {
        (
            format!("{title} (Incomplete recording)"),
            Some(format!("{reason}\n\n{}", summary.unwrap_or_default())),
        )
    } else {
        (title, summary)
    };
    let item = media::insert_item(
        &mut tx,
        &NewItem {
            library_id: library,
            kind: Some(if media.version.video_codec.is_some() {
                MediaKind::Movie
            } else {
                MediaKind::Track
            }),
            parent_id: None,
            title: &title,
            sort_title: &title.to_lowercase(),
            year: None,
            idx: None,
            parent_idx: None,
            originally_available_at: None,
            tmdb_id: None,
            tvdb_id: None,
            imdb_id: None,
            duration_ms: Some(media.duration_ms),
        },
        now,
    )
    .await?;
    // Use the snapshot taken when scheduling, even if guide maintenance removed the programme.
    sqlx::query("UPDATE media_items SET summary=?, content_rating=(SELECT content_rating FROM recordings WHERE id=?) WHERE id=?")
        .bind(summary).bind(id.0).bind(item.0).execute(&mut *tx).await?;
    let version = media::insert_version(&mut tx, item, &media.version).await?;
    let part = media::insert_part(
        &mut tx,
        &NewPart {
            version_id: version,
            path: media.path,
            size: media.size,
            mtime: media.mtime,
            quick_hash: None,
            duration_ms: Some(media.duration_ms),
            probe: media.probe,
            chapters: media.chapters,
            part_index: 0,
        },
    )
    .await?;
    media::replace_streams(&mut tx, part, &media.streams).await?;
    sqlx::query("UPDATE recordings SET item_id=?, library_id=?, output_path=?, size_bytes=?, duration_ms=?, status=?, error_message=? WHERE id=?")
        .bind(item.0).bind(library.0).bind(media.path).bind(media.size).bind(media.duration_ms)
        .bind(if media.incomplete_reason.is_some() { "partial" } else { "completed" })
        .bind(media.incomplete_reason).bind(id.0)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(item)
}
