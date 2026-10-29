//! Milestone M7 database repositories:
//! - Collections and collection items (`collections`, `collection_items`)
//! - Playlists and playlist items (`playlists`, `playlist_items`)
//! - Metadata field locks (`item_metadata_locks`)
//! - Music track metadata (MusicBrainz, ReplayGain, loudness, lyrics)
//! - Photos EXIF & timeline (`exif_date`, `camera_make`, `camera_model`, coordinates)
//! - Webhooks (`webhooks`)
//! - Media Requests & Sonarr/Radarr Bridge (`media_requests`)
//! - Movie Night Matchmaker (`matchmaker_sessions`, `matchmaker_votes`)
//! - Subsonic credentials (`subsonic_salt`, `subsonic_token`)

use mbl_core::{
    ArtworkId, CollectionId, Error, ItemId, LibraryId, PlaylistId, RequestId, Result, UserId,
    WebhookId, time::now_ms,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

use crate::Db;

// -----------------------------------------------------------------------------
// 1. Collections (M7.6)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRow {
    pub id: CollectionId,
    pub name: String,
    pub overview: Option<String>,
    pub poster_id: Option<ArtworkId>,
    pub backdrop_id: Option<ArtworkId>,
    pub is_smart: bool,
    pub smart_filter: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub item_count: i64,
}

pub async fn create_collection(
    db: &Db,
    name: &str,
    overview: Option<&str>,
    is_smart: bool,
    smart_filter: Option<&str>,
) -> Result<CollectionId> {
    let now = now_ms();
    let res = sqlx::query(
        r#"INSERT INTO collections (name, overview, is_smart, smart_filter, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(name)
    .bind(overview)
    .bind(if is_smart { 1 } else { 0 })
    .bind(smart_filter)
    .bind(now)
    .bind(now)
    .execute(&db.write)
    .await?;

    Ok(CollectionId(res.last_insert_rowid()))
}

pub async fn get_collection(db: &Db, id: CollectionId) -> Result<Option<CollectionRow>> {
    let row = sqlx::query(
        r#"SELECT c.id, c.name, c.overview, c.poster_id, c.backdrop_id, c.is_smart, c.smart_filter,
                  c.created_at, c.updated_at,
                  (SELECT COUNT(*) FROM collection_items ci WHERE ci.collection_id = c.id) as item_count
           FROM collections c
           WHERE c.id = ?"#,
    )
    .bind(id.0)
    .fetch_optional(&db.read)
    .await?;

    Ok(row.map(|r| CollectionRow {
        id: CollectionId(r.get("id")),
        name: r.get("name"),
        overview: r.get("overview"),
        poster_id: r.get::<Option<i64>, _>("poster_id").map(ArtworkId),
        backdrop_id: r.get::<Option<i64>, _>("backdrop_id").map(ArtworkId),
        is_smart: r.get::<i64, _>("is_smart") != 0,
        smart_filter: r.get("smart_filter"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        item_count: r.get("item_count"),
    }))
}

pub async fn find_collection_by_name(db: &Db, name: &str) -> Result<Option<CollectionRow>> {
    let row = sqlx::query(
        r#"SELECT c.id, c.name, c.overview, c.poster_id, c.backdrop_id, c.is_smart, c.smart_filter,
                  c.created_at, c.updated_at,
                  (SELECT COUNT(*) FROM collection_items ci WHERE ci.collection_id = c.id) as item_count
           FROM collections c
           WHERE c.name = ?"#,
    )
    .bind(name)
    .fetch_optional(&db.read)
    .await?;

    Ok(row.map(|r| CollectionRow {
        id: CollectionId(r.get("id")),
        name: r.get("name"),
        overview: r.get("overview"),
        poster_id: r.get::<Option<i64>, _>("poster_id").map(ArtworkId),
        backdrop_id: r.get::<Option<i64>, _>("backdrop_id").map(ArtworkId),
        is_smart: r.get::<i64, _>("is_smart") != 0,
        smart_filter: r.get("smart_filter"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        item_count: r.get("item_count"),
    }))
}

pub async fn list_collections(db: &Db) -> Result<Vec<CollectionRow>> {
    let rows = sqlx::query(
        r#"SELECT c.id, c.name, c.overview, c.poster_id, c.backdrop_id, c.is_smart, c.smart_filter,
                  c.created_at, c.updated_at,
                  (SELECT COUNT(*) FROM collection_items ci WHERE ci.collection_id = c.id) as item_count
           FROM collections c
           ORDER BY c.name COLLATE NOCASE ASC"#,
    )
    .fetch_all(&db.read)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| CollectionRow {
            id: CollectionId(r.get("id")),
            name: r.get("name"),
            overview: r.get("overview"),
            poster_id: r.get::<Option<i64>, _>("poster_id").map(ArtworkId),
            backdrop_id: r.get::<Option<i64>, _>("backdrop_id").map(ArtworkId),
            is_smart: r.get::<i64, _>("is_smart") != 0,
            smart_filter: r.get("smart_filter"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            item_count: r.get("item_count"),
        })
        .collect())
}

pub async fn update_collection(
    db: &Db,
    id: CollectionId,
    name: &str,
    overview: Option<&str>,
) -> Result<bool> {
    let now = now_ms();
    let res =
        sqlx::query("UPDATE collections SET name = ?, overview = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(overview)
            .bind(now)
            .bind(id.0)
            .execute(&db.write)
            .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn delete_collection(db: &Db, id: CollectionId) -> Result<bool> {
    let res =
        sqlx::query("DELETE FROM collections WHERE id = ?").bind(id.0).execute(&db.write).await?;
    Ok(res.rows_affected() > 0)
}

pub async fn add_item_to_collection(
    db: &Db,
    collection_id: CollectionId,
    item_id: ItemId,
) -> Result<()> {
    let max_order: Option<i64> =
        sqlx::query_scalar("SELECT MAX(sort_order) FROM collection_items WHERE collection_id = ?")
            .bind(collection_id.0)
            .fetch_optional(&db.read)
            .await?
            .flatten();

    let sort_order = max_order.unwrap_or(0) + 1;
    sqlx::query(
        r#"INSERT OR IGNORE INTO collection_items (collection_id, item_id, sort_order)
           VALUES (?, ?, ?)"#,
    )
    .bind(collection_id.0)
    .bind(item_id.0)
    .bind(sort_order)
    .execute(&db.write)
    .await?;

    Ok(())
}

pub async fn remove_item_from_collection(
    db: &Db,
    collection_id: CollectionId,
    item_id: ItemId,
) -> Result<bool> {
    let res = sqlx::query("DELETE FROM collection_items WHERE collection_id = ? AND item_id = ?")
        .bind(collection_id.0)
        .bind(item_id.0)
        .execute(&db.write)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn get_collection_item_ids(db: &Db, collection_id: CollectionId) -> Result<Vec<ItemId>> {
    let rows = sqlx::query(
        "SELECT item_id FROM collection_items WHERE collection_id = ? ORDER BY sort_order ASC",
    )
    .bind(collection_id.0)
    .fetch_all(&db.read)
    .await?;

    Ok(rows.into_iter().map(|r| ItemId(r.get("item_id"))).collect())
}

// -----------------------------------------------------------------------------
// 2. Playlists (M7.6)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistRow {
    pub id: PlaylistId,
    pub user_id: UserId,
    pub name: String,
    pub is_public: bool,
    pub is_smart: bool,
    pub smart_filter: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub item_count: i64,
}

pub async fn create_playlist(
    db: &Db,
    user_id: UserId,
    name: &str,
    is_public: bool,
    is_smart: bool,
    smart_filter: Option<&str>,
) -> Result<PlaylistId> {
    let now = now_ms();
    let res = sqlx::query(
        r#"INSERT INTO playlists (user_id, name, is_public, is_smart, smart_filter, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(user_id.0)
    .bind(name)
    .bind(if is_public { 1 } else { 0 })
    .bind(if is_smart { 1 } else { 0 })
    .bind(smart_filter)
    .bind(now)
    .bind(now)
    .execute(&db.write)
    .await?;

    Ok(PlaylistId(res.last_insert_rowid()))
}

pub async fn get_playlist(db: &Db, id: PlaylistId) -> Result<Option<PlaylistRow>> {
    let row = sqlx::query(
        r#"SELECT p.id, p.user_id, p.name, p.is_public, p.is_smart, p.smart_filter,
                  p.created_at, p.updated_at,
                  (SELECT COUNT(*) FROM playlist_items pi WHERE pi.playlist_id = p.id) as item_count
           FROM playlists p
           WHERE p.id = ?"#,
    )
    .bind(id.0)
    .fetch_optional(&db.read)
    .await?;

    Ok(row.map(|r| PlaylistRow {
        id: PlaylistId(r.get("id")),
        user_id: UserId(r.get("user_id")),
        name: r.get("name"),
        is_public: r.get::<i64, _>("is_public") != 0,
        is_smart: r.get::<i64, _>("is_smart") != 0,
        smart_filter: r.get("smart_filter"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        item_count: r.get("item_count"),
    }))
}

pub async fn list_playlists_for_user(db: &Db, user_id: UserId) -> Result<Vec<PlaylistRow>> {
    let rows = sqlx::query(
        r#"SELECT p.id, p.user_id, p.name, p.is_public, p.is_smart, p.smart_filter,
                  p.created_at, p.updated_at,
                  (SELECT COUNT(*) FROM playlist_items pi WHERE pi.playlist_id = p.id) as item_count
           FROM playlists p
           WHERE p.user_id = ? OR p.is_public = 1
           ORDER BY p.name COLLATE NOCASE ASC"#,
    )
    .bind(user_id.0)
    .fetch_all(&db.read)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PlaylistRow {
            id: PlaylistId(r.get("id")),
            user_id: UserId(r.get("user_id")),
            name: r.get("name"),
            is_public: r.get::<i64, _>("is_public") != 0,
            is_smart: r.get::<i64, _>("is_smart") != 0,
            smart_filter: r.get("smart_filter"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            item_count: r.get("item_count"),
        })
        .collect())
}

pub async fn update_playlist(db: &Db, id: PlaylistId, name: &str, is_public: bool) -> Result<bool> {
    let now = now_ms();
    let res =
        sqlx::query("UPDATE playlists SET name = ?, is_public = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(if is_public { 1 } else { 0 })
            .bind(now)
            .bind(id.0)
            .execute(&db.write)
            .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn delete_playlist(db: &Db, id: PlaylistId) -> Result<bool> {
    let res =
        sqlx::query("DELETE FROM playlists WHERE id = ?").bind(id.0).execute(&db.write).await?;
    Ok(res.rows_affected() > 0)
}

pub async fn add_item_to_playlist(db: &Db, playlist_id: PlaylistId, item_id: ItemId) -> Result<()> {
    let max_order: Option<i64> =
        sqlx::query_scalar("SELECT MAX(sort_order) FROM playlist_items WHERE playlist_id = ?")
            .bind(playlist_id.0)
            .fetch_optional(&db.read)
            .await?
            .flatten();

    let sort_order = max_order.unwrap_or(0) + 1;
    sqlx::query(
        r#"INSERT INTO playlist_items (playlist_id, item_id, sort_order)
           VALUES (?, ?, ?)"#,
    )
    .bind(playlist_id.0)
    .bind(item_id.0)
    .bind(sort_order)
    .execute(&db.write)
    .await?;

    Ok(())
}

pub async fn remove_item_from_playlist(
    db: &Db,
    playlist_id: PlaylistId,
    item_id: ItemId,
) -> Result<bool> {
    let res = sqlx::query("DELETE FROM playlist_items WHERE playlist_id = ? AND item_id = ?")
        .bind(playlist_id.0)
        .bind(item_id.0)
        .execute(&db.write)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn get_playlist_item_ids(db: &Db, playlist_id: PlaylistId) -> Result<Vec<ItemId>> {
    let rows = sqlx::query(
        "SELECT item_id FROM playlist_items WHERE playlist_id = ? ORDER BY sort_order ASC",
    )
    .bind(playlist_id.0)
    .fetch_all(&db.read)
    .await?;

    Ok(rows.into_iter().map(|r| ItemId(r.get("item_id"))).collect())
}

// -----------------------------------------------------------------------------
// 3. Metadata Field Locking (M7.7)
// -----------------------------------------------------------------------------

pub async fn get_locked_fields(db: &Db, item_id: ItemId) -> Result<Vec<String>> {
    let rows =
        sqlx::query("SELECT field FROM item_metadata_locks WHERE item_id = ? ORDER BY field ASC")
            .bind(item_id.0)
            .fetch_all(&db.read)
            .await?;

    Ok(rows.into_iter().map(|r| r.get("field")).collect())
}

pub async fn lock_field(db: &Db, item_id: ItemId, field: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO item_metadata_locks (item_id, field) VALUES (?, ?)")
        .bind(item_id.0)
        .bind(field)
        .execute(&db.write)
        .await?;
    Ok(())
}

pub async fn unlock_field(db: &Db, item_id: ItemId, field: &str) -> Result<bool> {
    let res = sqlx::query("DELETE FROM item_metadata_locks WHERE item_id = ? AND field = ?")
        .bind(item_id.0)
        .bind(field)
        .execute(&db.write)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn is_field_locked(db: &Db, item_id: ItemId, field: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM item_metadata_locks WHERE item_id = ? AND field = ?",
    )
    .bind(item_id.0)
    .bind(field)
    .fetch_one(&db.read)
    .await?;

    Ok(count > 0)
}

// -----------------------------------------------------------------------------
// 4. Track and Photo attributes (M7.1, M7.2, M7.5)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackAudioDetails {
    pub musicbrainz_id: Option<String>,
    pub disc_number: Option<i64>,
    pub track_number: Option<i64>,
    pub replaygain_track_gain: Option<f64>,
    pub replaygain_track_peak: Option<f64>,
    pub replaygain_album_gain: Option<f64>,
    pub replaygain_album_peak: Option<f64>,
    pub loudness_lufs: Option<f64>,
    pub loudness_range: Option<f64>,
    pub true_peak: Option<f64>,
    pub lyrics: Option<String>,
}

pub async fn update_track_details(
    db: &Db,
    item_id: ItemId,
    details: &TrackAudioDetails,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE media_items SET
             musicbrainz_id = COALESCE(?, musicbrainz_id),
             disc_number = COALESCE(?, disc_number),
             track_number = COALESCE(?, track_number),
             replaygain_track_gain = COALESCE(?, replaygain_track_gain),
             replaygain_track_peak = COALESCE(?, replaygain_track_peak),
             replaygain_album_gain = COALESCE(?, replaygain_album_gain),
             replaygain_album_peak = COALESCE(?, replaygain_album_peak),
             loudness_lufs = COALESCE(?, loudness_lufs),
             loudness_range = COALESCE(?, loudness_range),
             true_peak = COALESCE(?, true_peak),
             lyrics = COALESCE(?, lyrics)
           WHERE id = ?"#,
    )
    .bind(&details.musicbrainz_id)
    .bind(details.disc_number)
    .bind(details.track_number)
    .bind(details.replaygain_track_gain)
    .bind(details.replaygain_track_peak)
    .bind(details.replaygain_album_gain)
    .bind(details.replaygain_album_peak)
    .bind(details.loudness_lufs)
    .bind(details.loudness_range)
    .bind(details.true_peak)
    .bind(&details.lyrics)
    .bind(item_id.0)
    .execute(&db.write)
    .await?;

    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhotoExifDetails {
    pub exif_date: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

pub async fn update_photo_exif(db: &Db, item_id: ItemId, exif: &PhotoExifDetails) -> Result<()> {
    sqlx::query(
        r#"UPDATE media_items SET
             exif_date = COALESCE(?, exif_date),
             camera_make = COALESCE(?, camera_make),
             camera_model = COALESCE(?, camera_model),
             latitude = COALESCE(?, latitude),
             longitude = COALESCE(?, longitude)
           WHERE id = ?"#,
    )
    .bind(exif.exif_date)
    .bind(&exif.camera_make)
    .bind(&exif.camera_model)
    .bind(exif.latitude)
    .bind(exif.longitude)
    .bind(item_id.0)
    .execute(&db.write)
    .await?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PhotoTimelineBucket {
    pub year: i32,
    pub month: i32,
    pub count: i64,
}

pub async fn get_photo_timeline(
    db: &Db,
    library_id: LibraryId,
    filter: &super::items::ItemFilter,
) -> Result<Vec<PhotoTimelineBucket>> {
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "SELECT CAST(strftime('%Y', COALESCE(i.exif_date, i.added_at) / 1000, 'unixepoch') AS INTEGER) AS year, CAST(strftime('%m', COALESCE(i.exif_date, i.added_at) / 1000, 'unixepoch') AS INTEGER) AS month, COUNT(*) AS count FROM media_items i WHERE i.kind = 'photo' AND i.library_id = ",
    );
    query.push_bind(library_id.0);
    filter.push_acl(&mut query);
    query.push(" GROUP BY year, month ORDER BY year DESC, month DESC");
    let rows = query.build().fetch_all(&db.read).await?;

    Ok(rows
        .into_iter()
        .map(|r| PhotoTimelineBucket {
            year: r.get::<Option<i32>, _>("year").unwrap_or(1970),
            month: r.get::<Option<i32>, _>("month").unwrap_or(1),
            count: r.get("count"),
        })
        .collect())
}

// -----------------------------------------------------------------------------
// 5. Webhooks (M7.11)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRow {
    pub id: WebhookId,
    pub url: String,
    pub name: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
}

pub async fn create_webhook(
    db: &Db,
    url: &str,
    name: &str,
    events: &[String],
    secret: Option<&str>,
) -> Result<WebhookId> {
    let now = now_ms();
    let events_json = serde_json::to_string(events).unwrap_or_else(|_| "[]".into());
    let res = sqlx::query(
        "INSERT INTO webhooks (url, name, events, secret, enabled, created_at) VALUES (?, ?, ?, ?, 1, ?)",
    )
    .bind(url)
    .bind(name)
    .bind(events_json)
    .bind(secret)
    .bind(now)
    .execute(&db.write)
    .await?;

    Ok(WebhookId(res.last_insert_rowid()))
}

pub async fn list_webhooks(db: &Db) -> Result<Vec<WebhookRow>> {
    let rows = sqlx::query(
        "SELECT id, url, name, events, secret, enabled, created_at FROM webhooks ORDER BY id ASC",
    )
    .fetch_all(&db.read)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ev_str: String = r.get("events");
            let events: Vec<String> = serde_json::from_str(&ev_str).unwrap_or_default();
            WebhookRow {
                id: WebhookId(r.get("id")),
                url: r.get("url"),
                name: r.get("name"),
                events,
                secret: r.get("secret"),
                enabled: r.get::<i64, _>("enabled") != 0,
                created_at: r.get("created_at"),
            }
        })
        .collect())
}

pub async fn delete_webhook(db: &Db, id: WebhookId) -> Result<bool> {
    let res =
        sqlx::query("DELETE FROM webhooks WHERE id = ?").bind(id.0).execute(&db.write).await?;
    Ok(res.rows_affected() > 0)
}

pub async fn set_webhook_enabled(db: &Db, id: WebhookId, enabled: bool) -> Result<bool> {
    let res = sqlx::query("UPDATE webhooks SET enabled = ? WHERE id = ?")
        .bind(if enabled { 1 } else { 0 })
        .bind(id.0)
        .execute(&db.write)
        .await?;
    Ok(res.rows_affected() > 0)
}

// -----------------------------------------------------------------------------
// 6. Media Requests & Sonarr/Radarr Bridge (M7.20)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MediaRequestRow {
    pub id: RequestId,
    pub user_id: UserId,
    pub kind: String,
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub status: String,
    pub arr_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub async fn create_media_request(
    db: &Db,
    user_id: UserId,
    kind: &str,
    tmdb_id: i64,
    title: &str,
    year: Option<i32>,
) -> Result<RequestId> {
    let now = now_ms();
    let res = sqlx::query(
        r#"INSERT INTO media_requests (user_id, kind, tmdb_id, title, year, status, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, 'pending', ?, ?)"#,
    )
    .bind(user_id.0)
    .bind(kind)
    .bind(tmdb_id)
    .bind(title)
    .bind(year)
    .bind(now)
    .bind(now)
    .execute(&db.write)
    .await?;

    Ok(RequestId(res.last_insert_rowid()))
}

pub async fn list_media_requests(
    db: &Db,
    user_id: Option<UserId>,
    status: Option<&str>,
) -> Result<Vec<MediaRequestRow>> {
    let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "SELECT id, user_id, kind, tmdb_id, title, year, status, arr_id, created_at, updated_at FROM media_requests WHERE 1=1",
    );
    if let Some(uid) = user_id {
        qb.push(" AND user_id = ");
        qb.push_bind(uid.0);
    }
    if let Some(st) = status {
        qb.push(" AND status = ");
        qb.push_bind(st);
    }
    qb.push(" ORDER BY created_at DESC");

    let rows = qb.build().fetch_all(&db.read).await?;
    Ok(rows
        .into_iter()
        .map(|r| MediaRequestRow {
            id: RequestId(r.get("id")),
            user_id: UserId(r.get("user_id")),
            kind: r.get("kind"),
            tmdb_id: r.get("tmdb_id"),
            title: r.get("title"),
            year: r.get("year"),
            status: r.get("status"),
            arr_id: r.get("arr_id"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn update_media_request_status(
    db: &Db,
    id: RequestId,
    status: &str,
    arr_id: Option<i64>,
) -> Result<bool> {
    let now = now_ms();
    let res = sqlx::query(
        "UPDATE media_requests SET status = ?, arr_id = COALESCE(?, arr_id), updated_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(arr_id)
    .bind(now)
    .bind(id.0)
    .execute(&db.write)
    .await?;

    Ok(res.rows_affected() > 0)
}

// -----------------------------------------------------------------------------
// 7. Movie Night Matchmaker (M7.19)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerSessionRow {
    pub id: String,
    pub created_by: UserId,
    pub filter_config: String,
    pub status: String,
    pub matched_item_id: Option<ItemId>,
    pub created_at: i64,
}

pub async fn create_matchmaker_session(
    db: &Db,
    id: &str,
    created_by: UserId,
    filter_config: &str,
) -> Result<()> {
    let now = now_ms();
    let mut tx = db.write.begin().await?;
    sqlx::query(
        "INSERT INTO matchmaker_sessions (id, created_by, filter_config, status, created_at) VALUES (?, ?, ?, 'active', ?)",
    )
    .bind(id)
    .bind(created_by.0)
    .bind(filter_config)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO matchmaker_members (session_id, user_id, joined_at) VALUES (?, ?, ?)")
        .bind(id)
        .bind(created_by.0)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn get_matchmaker_session(db: &Db, id: &str) -> Result<Option<MatchmakerSessionRow>> {
    let row = sqlx::query(
        "SELECT id, created_by, filter_config, status, matched_item_id, created_at FROM matchmaker_sessions WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&db.read)
    .await?;

    Ok(row.map(|r| MatchmakerSessionRow {
        id: r.get("id"),
        created_by: UserId(r.get("created_by")),
        filter_config: r.get("filter_config"),
        status: r.get("status"),
        matched_item_id: r.get::<Option<i64>, _>("matched_item_id").map(ItemId),
        created_at: r.get("created_at"),
    }))
}

pub async fn record_matchmaker_vote(
    db: &Db,
    session_id: &str,
    user_name: &str,
    item_id: ItemId,
    vote: i32,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO matchmaker_votes (session_id, user_name, item_id, vote)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(session_id, user_name, item_id) DO UPDATE SET vote = excluded.vote"#,
    )
    .bind(session_id)
    .bind(user_name)
    .bind(item_id.0)
    .bind(vote)
    .execute(&db.write)
    .await?;

    Ok(())
}

pub async fn check_matchmaker_matches(
    db: &Db,
    session_id: &str,
    required_participants: usize,
) -> Result<Option<ItemId>> {
    if required_participants == 0 {
        return Ok(None);
    }

    // Find any item where positive votes (vote = 1) equals or exceeds required participants
    let item: Option<i64> = sqlx::query_scalar(
        r#"SELECT item_id
           FROM matchmaker_votes
           WHERE session_id = ? AND vote = 1
           GROUP BY item_id
           HAVING COUNT(DISTINCT user_name) >= ?
           LIMIT 1"#,
    )
    .bind(session_id)
    .bind(required_participants as i64)
    .fetch_optional(&db.read)
    .await?;

    Ok(item.map(ItemId))
}

pub async fn complete_matchmaker_session(
    db: &Db,
    session_id: &str,
    matched_item_id: ItemId,
) -> Result<()> {
    sqlx::query(
        "UPDATE matchmaker_sessions SET status = 'matched', matched_item_id = ? WHERE id = ?",
    )
    .bind(matched_item_id.0)
    .bind(session_id)
    .execute(&db.write)
    .await?;

    Ok(())
}

// -----------------------------------------------------------------------------
// 8. Subsonic credentials (M7.4)
// -----------------------------------------------------------------------------

pub async fn set_subsonic_credentials(
    db: &Db,
    user_id: UserId,
    salt: &str,
    token: &str,
) -> Result<()> {
    sqlx::query("UPDATE users SET subsonic_salt = ?, subsonic_token = ? WHERE id = ?")
        .bind(salt)
        .bind(token)
        .bind(user_id.0)
        .execute(&db.write)
        .await?;
    Ok(())
}

pub async fn get_subsonic_credentials(
    db: &Db,
    username: &str,
) -> Result<Option<(UserId, Option<String>, Option<String>, String)>> {
    // Returns (user_id, subsonic_salt, subsonic_token, password_hash)
    let row = sqlx::query(
        "SELECT id, subsonic_salt, subsonic_token, password_hash FROM users WHERE username = ? COLLATE NOCASE",
    )
    .bind(username)
    .fetch_optional(&db.read)
    .await?;

    Ok(row.map(|r| {
        (
            UserId(r.get("id")),
            r.get("subsonic_salt"),
            r.get("subsonic_token"),
            r.get("password_hash"),
        )
    }))
}

/// Joining requires the unguessable session link and an active authenticated identity.
pub async fn join_matchmaker(db: &Db, id: &str, user: UserId) -> Result<()> {
    let mut tx = db.write.begin().await?;
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM matchmaker_sessions WHERE id = ? AND status = 'active' AND created_at > ?)")
        .bind(id).bind(now_ms() - 86_400_000).fetch_one(&mut *tx).await?;
    if !active {
        return Err(Error::NotFound);
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM matchmaker_members WHERE session_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if count >= 16 {
        return Err(Error::Conflict("matchmaker lobby is full".into()));
    }
    sqlx::query("INSERT OR IGNORE INTO matchmaker_members (session_id, user_id, joined_at) VALUES (?, ?, ?)")
        .bind(id).bind(user.0).bind(now_ms()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn is_matchmaker_member(db: &Db, id: &str, user: UserId) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM matchmaker_members WHERE session_id = ? AND user_id = ?)",
    )
    .bind(id)
    .bind(user.0)
    .fetch_one(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn matchmaker_member_count(db: &Db, id: &str) -> Result<usize> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM matchmaker_members WHERE session_id = ?")
            .bind(id)
            .fetch_one(&db.read)
            .await?;
    Ok(count as usize)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PhotoSummary {
    pub id: ItemId,
    pub title: String,
    pub taken_at: i64,
    pub part_id: Option<mbl_core::PartId>,
}

pub async fn list_photos(
    db: &Db,
    library: LibraryId,
    filter: &super::items::ItemFilter,
    year: Option<i32>,
    month: Option<u32>,
    cursor: Option<&str>,
    limit: u32,
) -> Result<mbl_core::Paginated<PhotoSummary>> {
    if year.is_some() != month.is_some()
        || year.is_some_and(|n| !(1..=9999).contains(&n))
        || month.is_some_and(|n| !(1..=12).contains(&n))
    {
        return Err(Error::Validation("year and month must be a valid pair".into()));
    }
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "SELECT i.id, i.title, COALESCE(i.exif_date, i.added_at) AS taken_at, (SELECT p.id FROM media_versions v JOIN media_parts p ON p.version_id = v.id WHERE v.item_id = i.id AND p.missing_since IS NULL ORDER BY v.id, p.part_index LIMIT 1) AS part_id FROM media_items i WHERE i.kind = 'photo' AND i.library_id = ",
    );
    query.push_bind(library.0);
    filter.push_acl(&mut query);
    if let (Some(year), Some(month)) = (year, month) {
        query.push(
            " AND strftime('%Y-%m', COALESCE(i.exif_date, i.added_at) / 1000, 'unixepoch') = ",
        );
        query.push_bind(format!("{year:04}-{month:02}"));
    }
    if let Some(cursor) = cursor {
        let (date, id) = cursor
            .split_once(':')
            .ok_or_else(|| Error::Validation("invalid photo cursor".into()))?;
        let date: i64 =
            date.parse().map_err(|_| Error::Validation("invalid photo cursor".into()))?;
        let id: i64 = id.parse().map_err(|_| Error::Validation("invalid photo cursor".into()))?;
        query
            .push(" AND (COALESCE(i.exif_date, i.added_at) < ")
            .push_bind(date)
            .push(" OR (COALESCE(i.exif_date, i.added_at) = ")
            .push_bind(date)
            .push(" AND i.id < ")
            .push_bind(id)
            .push("))");
    }
    let limit = limit.clamp(1, 100) as usize;
    query.push(" ORDER BY taken_at DESC, i.id DESC LIMIT ").push_bind((limit + 1) as i64);
    let mut items = query.build_query_as::<PhotoSummary>().fetch_all(&db.read).await?;
    let more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = if more {
        items.last().map(|item| mbl_core::Cursor(format!("{}:{}", item.taken_at, item.id.0)))
    } else {
        None
    };
    Ok(mbl_core::Paginated { items, next_cursor, total: None })
}
