//! Atomic publication of finalized recordings into the ordinary playback catalogue.
use crate::{
    Db,
    repo::media::{self, NewItem, NewPart, NewStream, VersionSummary},
};
use mbl_core::{Error, ItemId, LibraryId, MediaKind, RecordingId, Result};

pub struct RecordedMedia<'a> {
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

pub async fn publish(
    db: &Db,
    id: RecordingId,
    media: &RecordedMedia<'_>,
    now: i64,
) -> Result<ItemId> {
    let mut tx = db.write.begin().await?;
    // This transaction also serializes cancellation and competing publication attempts.
    let row: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT title, item_id FROM recordings WHERE id=? AND status IN ('recording','completed')",
    )
    .bind(id.0)
    .fetch_optional(&mut *tx)
    .await?;
    let (title, existing) = row.ok_or(Error::NotFound)?;
    if let Some(item) = existing {
        return Ok(ItemId(item));
    }
    let library: Option<i64> =
        sqlx::query_scalar("SELECT library_id FROM dvr_libraries WHERE path=?")
            .bind(media.root)
            .fetch_optional(&mut *tx)
            .await?;
    let library = match library {
        Some(id) => LibraryId(id),
        None => {
            let id: i64 = sqlx::query_scalar("INSERT INTO libraries (name,kind,created_at) VALUES ('TV recordings','homevideo',?) RETURNING id")
                .bind(now).fetch_one(&mut *tx).await?;
            sqlx::query("INSERT INTO library_paths (library_id,path) VALUES (?,?)")
                .bind(id)
                .bind(media.root)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO dvr_libraries (library_id,path) VALUES (?,?)")
                .bind(id)
                .bind(media.root)
                .execute(&mut *tx)
                .await?;
            LibraryId(id)
        }
    };
    let item = media::insert_item(
        &mut tx,
        &NewItem {
            library_id: library,
            kind: Some(MediaKind::Movie),
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
    sqlx::query("UPDATE media_items SET summary=(SELECT summary FROM recordings WHERE id=?), content_rating=(SELECT content_rating FROM recordings WHERE id=?) WHERE id=?")
        .bind(id.0).bind(id.0).bind(item.0).execute(&mut *tx).await?;
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
    sqlx::query("UPDATE recordings SET status='completed', item_id=?, output_path=?, size_bytes=?, duration_ms=?, error_message=NULL WHERE id=?")
        .bind(item.0).bind(media.path).bind(media.size).bind(media.duration_ms).bind(id.0)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(item)
}
