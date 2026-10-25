//! Media items, versions, parts and streams. Write helpers take a connection so the scanner can
//! batch many files into one transaction on the write pool.

use mbl_core::{ItemId, LibraryId, MediaKind, PartId, Result, StreamKind, VersionId};
use sqlx::SqliteConnection;

use crate::Db;

/// A known file on disk, for change detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartFile {
    pub id: PartId,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    pub quick_hash: Option<Vec<u8>>,
    pub missing_since: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewItem<'a> {
    pub library_id: LibraryId,
    pub kind: Option<MediaKind>,
    pub parent_id: Option<ItemId>,
    pub title: &'a str,
    pub sort_title: &'a str,
    pub year: Option<i32>,
    pub idx: Option<i64>,
    pub parent_idx: Option<i64>,
    pub originally_available_at: Option<&'a str>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    pub imdb_id: Option<&'a str>,
    pub duration_ms: Option<i64>,
}

/// Technical summary of a version, copied from the probe of its first part.
#[derive(Debug, Clone, Default)]
pub struct VersionSummary<'a> {
    pub edition: Option<&'a str>,
    pub container: Option<&'a str>,
    pub bitrate: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<&'a str>,
    pub video_profile: Option<&'a str>,
    pub bit_depth: Option<i64>,
    pub hdr: Option<&'a str>,
    pub audio_codec: Option<&'a str>,
    pub audio_channels: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewPart<'a> {
    pub version_id: VersionId,
    pub path: &'a str,
    pub size: i64,
    pub mtime: i64,
    pub quick_hash: Option<&'a [u8]>,
    pub duration_ms: Option<i64>,
    pub probe: &'a str,
    pub chapters: &'a str,
    pub part_index: i64,
}

#[derive(Debug, Clone, Default)]
pub struct NewStream<'a> {
    pub idx: i64,
    pub kind: &'a str,
    pub codec: &'a str,
    pub profile: Option<&'a str>,
    pub language: Option<&'a str>,
    pub title: Option<&'a str>,
    pub is_default: bool,
    pub is_forced: bool,
    pub external_path: Option<&'a str>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub bit_depth: Option<i64>,
    pub hdr: Option<&'a str>,
    pub channels: Option<i64>,
    pub channel_layout: Option<&'a str>,
}

/// Every part belonging to a library (including missing ones).
pub async fn parts_for_library(db: &Db, library_id: LibraryId) -> Result<Vec<PartFile>> {
    Ok(sqlx::query_as!(
        PartFile,
        r#"SELECT p.id AS "id: PartId", p.path, p.size, p.mtime, p.quick_hash, p.missing_since
           FROM media_parts p
           JOIN media_versions v ON v.id = p.version_id
           JOIN media_items i ON i.id = v.item_id
           WHERE i.library_id = ?"#,
        library_id
    )
    .fetch_all(&db.read)
    .await?)
}

/// Top-level item (movie, show, artist) by title/year, or by TMDB id when given.
pub async fn find_top_item(
    conn: &mut SqliteConnection,
    library_id: LibraryId,
    kind: MediaKind,
    title: &str,
    year: Option<i32>,
    tmdb_id: Option<i64>,
) -> Result<Option<ItemId>> {
    if let Some(tmdb) = tmdb_id {
        let found = sqlx::query_scalar!(
            r#"SELECT id AS "id: ItemId" FROM media_items
               WHERE library_id = ? AND kind = ? AND parent_id IS NULL AND tmdb_id = ? LIMIT 1"#,
            library_id,
            kind,
            tmdb
        )
        .fetch_optional(&mut *conn)
        .await?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(sqlx::query_scalar!(
        r#"SELECT id AS "id: ItemId" FROM media_items
           WHERE library_id = ? AND kind = ? AND parent_id IS NULL
             AND title = ? COLLATE NOCASE AND year IS ? LIMIT 1"#,
        library_id,
        kind,
        title,
        year
    )
    .fetch_optional(&mut *conn)
    .await?)
}

/// Child (season, episode, album, track) by index, or by title when `idx` is `None`.
pub async fn find_child(
    conn: &mut SqliteConnection,
    parent_id: ItemId,
    kind: MediaKind,
    idx: Option<i64>,
    title: &str,
) -> Result<Option<ItemId>> {
    Ok(match idx {
        Some(idx) => {
            sqlx::query_scalar!(
                r#"SELECT id AS "id: ItemId" FROM media_items
               WHERE parent_id = ? AND kind = ? AND idx = ? LIMIT 1"#,
                parent_id,
                kind,
                idx
            )
            .fetch_optional(&mut *conn)
            .await?
        }
        None => {
            sqlx::query_scalar!(
                r#"SELECT id AS "id: ItemId" FROM media_items
               WHERE parent_id = ? AND kind = ? AND title = ? COLLATE NOCASE LIMIT 1"#,
                parent_id,
                kind,
                title
            )
            .fetch_optional(&mut *conn)
            .await?
        }
    })
}

pub async fn insert_item(
    conn: &mut SqliteConnection,
    item: &NewItem<'_>,
    now: i64,
) -> Result<ItemId> {
    let kind = item.kind.unwrap_or(MediaKind::Movie);
    Ok(sqlx::query_scalar!(
        r#"INSERT INTO media_items
             (library_id, kind, parent_id, title, sort_title, year, idx, parent_idx,
              originally_available_at, tmdb_id, tvdb_id, imdb_id, duration_ms, added_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!: ItemId""#,
        item.library_id,
        kind,
        item.parent_id,
        item.title,
        item.sort_title,
        item.year,
        item.idx,
        item.parent_idx,
        item.originally_available_at,
        item.tmdb_id,
        item.tvdb_id,
        item.imdb_id,
        item.duration_ms,
        now,
        now
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// An existing version of `item_id` with the given edition whose parts live in `dir`
/// (used to attach `cd2` to the version that already holds `cd1`).
pub async fn find_multipart_version(
    conn: &mut SqliteConnection,
    item_id: ItemId,
    edition: Option<&str>,
    dir_prefix: &str,
) -> Result<Option<VersionId>> {
    let pattern = format!("{}%", dir_prefix.replace('%', "\\%").replace('_', "\\_"));
    Ok(sqlx::query_scalar!(
        r#"SELECT v.id AS "id: VersionId" FROM media_versions v
           JOIN media_parts p ON p.version_id = v.id
           WHERE v.item_id = ? AND v.edition IS ? AND p.part_index > 0 AND p.path LIKE ? ESCAPE '\'
           LIMIT 1"#,
        item_id,
        edition,
        pattern
    )
    .fetch_optional(&mut *conn)
    .await?)
}

pub async fn insert_version(
    conn: &mut SqliteConnection,
    item_id: ItemId,
    v: &VersionSummary<'_>,
) -> Result<VersionId> {
    let id = sqlx::query_scalar!(
        r#"INSERT INTO media_versions
             (item_id, edition, container, bitrate, width, height, video_codec, video_profile,
              bit_depth, hdr, audio_codec, audio_channels)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!: VersionId""#,
        item_id,
        v.edition,
        v.container,
        v.bitrate,
        v.width,
        v.height,
        v.video_codec,
        v.video_profile,
        v.bit_depth,
        v.hdr,
        v.audio_codec,
        v.audio_channels
    )
    .fetch_one(&mut *conn)
    .await?;
    link_item_version(conn, item_id, id).await?;
    Ok(id)
}

pub async fn update_version_summary(
    conn: &mut SqliteConnection,
    id: VersionId,
    v: &VersionSummary<'_>,
) -> Result<()> {
    sqlx::query!(
        r#"UPDATE media_versions SET container = ?, bitrate = ?, width = ?, height = ?,
             video_codec = ?, video_profile = ?, bit_depth = ?, hdr = ?, audio_codec = ?,
             audio_channels = ?
           WHERE id = ?"#,
        v.container,
        v.bitrate,
        v.width,
        v.height,
        v.video_codec,
        v.video_profile,
        v.bit_depth,
        v.hdr,
        v.audio_codec,
        v.audio_channels,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Link an item to a version (multi-episode files share one version).
pub async fn link_item_version(
    conn: &mut SqliteConnection,
    item_id: ItemId,
    version_id: VersionId,
) -> Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO item_versions (item_id, version_id) VALUES (?, ?)",
        item_id,
        version_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn insert_part(conn: &mut SqliteConnection, p: &NewPart<'_>) -> Result<PartId> {
    Ok(sqlx::query_scalar!(
        r#"INSERT INTO media_parts
             (version_id, path, size, mtime, quick_hash, duration_ms, probe, chapters, part_index)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!: PartId""#,
        p.version_id,
        p.path,
        p.size,
        p.mtime,
        p.quick_hash,
        p.duration_ms,
        p.probe,
        p.chapters,
        p.part_index
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// New file facts for a part whose file changed in place.
#[derive(Debug, Clone)]
pub struct PartUpdate<'a> {
    pub size: i64,
    pub mtime: i64,
    pub quick_hash: Option<&'a [u8]>,
    pub duration_ms: Option<i64>,
    pub probe: &'a str,
    pub chapters: &'a str,
}

/// Refresh a part whose file changed in place; returns its version.
pub async fn update_part_probe(
    conn: &mut SqliteConnection,
    id: PartId,
    u: &PartUpdate<'_>,
) -> Result<VersionId> {
    Ok(sqlx::query_scalar!(
        r#"UPDATE media_parts SET size = ?, mtime = ?, quick_hash = ?, duration_ms = ?, probe = ?,
             chapters = ?, missing_since = NULL
           WHERE id = ? RETURNING version_id AS "version_id!: VersionId""#,
        u.size,
        u.mtime,
        u.quick_hash,
        u.duration_ms,
        u.probe,
        u.chapters,
        id
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// A file was renamed/moved: keep the part (and its item, watch state, ...) and update its path.
pub async fn move_part(
    conn: &mut SqliteConnection,
    id: PartId,
    path: &str,
    mtime: i64,
) -> Result<()> {
    sqlx::query!(
        "UPDATE media_parts SET path = ?, mtime = ?, missing_since = NULL WHERE id = ?",
        path,
        mtime,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn replace_streams(
    conn: &mut SqliteConnection,
    part_id: PartId,
    streams: &[NewStream<'_>],
) -> Result<()> {
    sqlx::query!("DELETE FROM streams WHERE part_id = ?", part_id).execute(&mut *conn).await?;
    for s in streams {
        sqlx::query!(
            r#"INSERT INTO streams
                 (part_id, idx, kind, codec, profile, language, title, is_default, is_forced,
                  external_path, width, height, bit_depth, hdr, channels, channel_layout)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            part_id,
            s.idx,
            s.kind,
            s.codec,
            s.profile,
            s.language,
            s.title,
            s.is_default,
            s.is_forced,
            s.external_path,
            s.width,
            s.height,
            s.bit_depth,
            s.hdr,
            s.channels,
            s.channel_layout
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn set_missing(conn: &mut SqliteConnection, id: PartId, now: i64) -> Result<()> {
    sqlx::query!(
        "UPDATE media_parts SET missing_since = ? WHERE id = ? AND missing_since IS NULL",
        now,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn clear_missing(conn: &mut SqliteConnection, id: PartId) -> Result<()> {
    sqlx::query!("UPDATE media_parts SET missing_since = NULL WHERE id = ?", id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn touch_library_paths(db: &Db, library_id: LibraryId, now: i64) -> Result<()> {
    sqlx::query!("UPDATE library_paths SET last_scan_at = ? WHERE library_id = ?", now, library_id)
        .execute(&db.write)
        .await?;
    Ok(())
}

/// Library an item belongs to, if the item exists.
pub async fn library_of_item(db: &Db, id: ItemId) -> Result<Option<LibraryId>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT library_id AS "library_id: LibraryId" FROM media_items WHERE id = ?"#,
        id
    )
    .fetch_optional(&db.read)
    .await?)
}

/// Item counts by kind, for tests and stats.
pub async fn count_items(db: &Db, library_id: LibraryId, kind: MediaKind) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM media_items WHERE library_id = ? AND kind = ?",
        library_id,
        kind
    )
    .fetch_one(&db.read)
    .await?)
}

/// Part metadata needed for streaming and playback decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPart {
    pub id: PartId,
    pub version_id: VersionId,
    pub item_id: ItemId,
    pub library_id: LibraryId,
    pub path: String,
    pub size: i64,
    pub duration_ms: Option<i64>,
    pub container: Option<String>,
    pub probe: String,
    pub missing_since: Option<i64>,
}

/// Look up a single part and its owning item/library for streaming.
pub async fn part_for_stream(db: &Db, part_id: PartId) -> Result<Option<StreamPart>> {
    Ok(sqlx::query_as!(
        StreamPart,
        r#"SELECT p.id AS "id!: PartId",
                  p.version_id AS "version_id!: VersionId",
                  i.id AS "item_id!: ItemId",
                  i.library_id AS "library_id!: LibraryId",
                  p.path,
                  p.size,
                  p.duration_ms,
                  v.container,
                  p.probe,
                  p.missing_since
           FROM media_parts p
           JOIN media_versions v ON v.id = p.version_id
           JOIN media_items i ON i.id = v.item_id
           WHERE p.id = ?"#,
        part_id
    )
    .fetch_optional(&db.read)
    .await?)
}

/// Look up all parts for a media item (including multi-episode shared versions).
pub async fn parts_for_item(db: &Db, item_id: ItemId) -> Result<Vec<StreamPart>> {
    Ok(sqlx::query_as!(
        StreamPart,
        r#"SELECT p.id AS "id!: PartId",
                  p.version_id AS "version_id!: VersionId",
                  i.id AS "item_id!: ItemId",
                  i.library_id AS "library_id!: LibraryId",
                  p.path,
                  p.size,
                  p.duration_ms,
                  v.container,
                  p.probe,
                  p.missing_since
           FROM media_parts p
           JOIN media_versions v ON v.id = p.version_id
           JOIN media_items i ON i.id = ?1
           WHERE v.item_id = ?1
              OR v.id IN (SELECT version_id FROM item_versions WHERE item_id = ?1)
           ORDER BY v.is_optimized DESC, v.id, p.part_index, p.id"#,
        item_id
    )
    .fetch_all(&db.read)
    .await?)
}

/// Stream row stored in `streams`, including `external_path` for sidecar subtitles.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PartStream {
    pub idx: i64,
    pub kind: StreamKind,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub external_path: Option<String>,
}

/// Look up all streams for a media part ordered by stream index.
pub async fn streams_for_part(db: &Db, part_id: PartId) -> Result<Vec<PartStream>> {
    Ok(sqlx::query_as::<_, PartStream>(
        "SELECT idx, kind, codec, language, title, is_default, is_forced, external_path
         FROM streams WHERE part_id = ? ORDER BY idx",
    )
    .bind(part_id)
    .fetch_all(&db.read)
    .await?)
}

/// Fetch the raw `chapters` JSON string for `part_id`.
pub async fn part_chapters(db: &Db, part_id: PartId) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT chapters FROM media_parts WHERE id = ?")
        .bind(part_id)
        .fetch_optional(&db.read)
        .await?)
}
