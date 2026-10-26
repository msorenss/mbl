//! Milestone M9 Repository queries: Live TV, Channels, EPG, Recordings, Plugins, and Virtual Channels.

use mbl_core::ids::{
    ChannelId, ItemId, PluginId, ProgramId, RecordingId, RecordingRuleId, TunerId, UserId,
    VirtualChannelId,
};
use mbl_core::time::now_ms;
use mbl_core::{Error, Result, VirtualChannelItem};
use sqlx::{FromRow, Sqlite};

use crate::Db;

#[derive(Debug, Clone, FromRow)]
pub struct TunerRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub url: String,
    pub status: String,
    pub tuner_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct ChannelRow {
    pub id: i64,
    pub tuner_id: Option<i64>,
    pub channel_number: String,
    pub name: String,
    pub callsign: Option<String>,
    pub logo_url: Option<String>,
    pub stream_url: String,
    pub epg_channel_id: Option<String>,
    pub is_favorite: i64,
    pub is_hidden: i64,
    pub is_virtual: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct EpgProgramRow {
    pub id: i64,
    pub channel_id: Option<i64>,
    pub epg_channel_id: String,
    pub title: String,
    pub description: Option<String>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub start_time: i64,
    pub end_time: i64,
    pub icon_url: Option<String>,
    pub rating: Option<String>,
    pub genres: String,
    pub is_new: i64,
}

#[derive(Debug, Clone)]
pub struct EpgProgramInsert {
    pub epg_channel_id: String,
    pub channel_id: Option<ChannelId>,
    pub title: String,
    pub description: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub start_time: i64,
    pub end_time: i64,
    pub icon_url: Option<String>,
    pub rating: Option<String>,
    pub genres: Vec<String>,
    pub is_new: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct RecordingRuleRow {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub channel_id: Option<i64>,
    pub series_only: i64,
    pub season_number: Option<i64>,
    pub new_only: i64,
    pub start_padding_secs: i64,
    pub end_padding_secs: i64,
    pub keep_max: Option<i64>,
    pub auto_delete_watched: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct RecordingRow {
    pub id: i64,
    pub rule_id: Option<i64>,
    pub channel_id: i64,
    pub channel_name: Option<String>,
    pub program_id: Option<i64>,
    pub title: String,
    pub start_time: i64,
    pub end_time: i64,
    pub status: String,
    pub output_path: Option<String>,
    pub size_bytes: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct PluginConfigRow {
    pub id: i64,
    pub plugin_id: String,
    pub name: String,
    pub enabled: i64,
    pub settings: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct VirtualChannelRow {
    pub id: i64,
    pub channel_id: i64,
    pub name: String,
    pub mode: String,
    pub items: String,
    pub insert_commercials: i64,
    pub total_duration_ms: i64,
    pub created_at: i64,
}

// ---------------------------------------------------------------------------
// Tuners
// ---------------------------------------------------------------------------

pub async fn list_tuners(db: &Db) -> Result<Vec<TunerRow>> {
    sqlx::query_as::<Sqlite, TunerRow>(
        r#"SELECT id, name, kind, url, status, tuner_count, created_at, updated_at
           FROM tuners
           ORDER BY name ASC"#,
    )
    .fetch_all(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn get_tuner(db: &Db, id: TunerId) -> Result<Option<TunerRow>> {
    sqlx::query_as::<Sqlite, TunerRow>(
        r#"SELECT id, name, kind, url, status, tuner_count, created_at, updated_at
           FROM tuners
           WHERE id = ?"#,
    )
    .bind(id.0)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn create_tuner(
    db: &Db,
    name: &str,
    kind: &str,
    url: &str,
    tuner_count: u32,
) -> Result<TunerId> {
    let now = now_ms();
    let res = sqlx::query(
        r#"INSERT INTO tuners (name, kind, url, status, tuner_count, created_at, updated_at)
           VALUES (?, ?, ?, 'online', ?, ?, ?)
           RETURNING id"#,
    )
    .bind(name)
    .bind(kind)
    .bind(url)
    .bind(tuner_count as i64)
    .bind(now)
    .bind(now)
    .fetch_one(&db.write)
    .await
    .map_err(Error::from)?;

    use sqlx::Row;
    let id: i64 = res.get(0);
    Ok(TunerId(id))
}

pub async fn delete_tuner(db: &Db, id: TunerId) -> Result<bool> {
    sqlx::query("DELETE FROM channels WHERE tuner_id = ?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    let res = sqlx::query("DELETE FROM tuners WHERE id = ?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    Ok(res.rows_affected() > 0)
}

pub async fn update_tuner_status(db: &Db, id: TunerId, status: &str) -> Result<()> {
    let now = now_ms();
    sqlx::query("UPDATE tuners SET status = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(now)
        .bind(id.0)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

pub async fn list_channels(db: &Db, include_hidden: bool) -> Result<Vec<ChannelRow>> {
    let sql = if include_hidden {
        r#"SELECT id, tuner_id, channel_number, name, callsign, logo_url,
                  stream_url, epg_channel_id, is_favorite, is_hidden, is_virtual,
                  created_at, updated_at
           FROM channels
           ORDER BY CAST(channel_number AS REAL) ASC, channel_number ASC"#
    } else {
        r#"SELECT id, tuner_id, channel_number, name, callsign, logo_url,
                  stream_url, epg_channel_id, is_favorite, is_hidden, is_virtual,
                  created_at, updated_at
           FROM channels
           WHERE is_hidden = 0
           ORDER BY CAST(channel_number AS REAL) ASC, channel_number ASC"#
    };

    sqlx::query_as::<Sqlite, ChannelRow>(sql).fetch_all(&db.read).await.map_err(Error::from)
}

pub async fn get_channel(db: &Db, id: ChannelId) -> Result<Option<ChannelRow>> {
    sqlx::query_as::<Sqlite, ChannelRow>(
        r#"SELECT id, tuner_id, channel_number, name, callsign, logo_url,
                  stream_url, epg_channel_id, is_favorite, is_hidden, is_virtual,
                  created_at, updated_at
           FROM channels
           WHERE id = ?"#,
    )
    .bind(id.0)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_channel(
    db: &Db,
    tuner_id: Option<TunerId>,
    channel_number: &str,
    name: &str,
    callsign: Option<&str>,
    logo_url: Option<&str>,
    stream_url: &str,
    epg_channel_id: Option<&str>,
    is_virtual: bool,
) -> Result<ChannelId> {
    let now = now_ms();
    let res = sqlx::query(
        r#"INSERT INTO channels (
               tuner_id, channel_number, name, callsign, logo_url,
               stream_url, epg_channel_id, is_virtual, created_at, updated_at
           )
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           RETURNING id"#,
    )
    .bind(tuner_id.map(|t| t.0))
    .bind(channel_number)
    .bind(name)
    .bind(callsign)
    .bind(logo_url)
    .bind(stream_url)
    .bind(epg_channel_id)
    .bind(if is_virtual { 1i64 } else { 0i64 })
    .bind(now)
    .bind(now)
    .fetch_one(&db.write)
    .await
    .map_err(Error::from)?;

    use sqlx::Row;
    let id: i64 = res.get(0);
    Ok(ChannelId(id))
}

pub async fn update_channel(
    db: &Db,
    id: ChannelId,
    name: Option<&str>,
    channel_number: Option<&str>,
    epg_channel_id: Option<Option<&str>>,
    is_favorite: Option<bool>,
    is_hidden: Option<bool>,
) -> Result<bool> {
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new("UPDATE channels SET updated_at = ");
    qb.push_bind(now_ms());

    if let Some(n) = name {
        qb.push(", name = ");
        qb.push_bind(n);
    }
    if let Some(cn) = channel_number {
        qb.push(", channel_number = ");
        qb.push_bind(cn);
    }
    if let Some(epg) = epg_channel_id {
        qb.push(", epg_channel_id = ");
        qb.push_bind(epg);
    }
    if let Some(fav) = is_favorite {
        qb.push(", is_favorite = ");
        qb.push_bind(if fav { 1i64 } else { 0i64 });
    }
    if let Some(hid) = is_hidden {
        qb.push(", is_hidden = ");
        qb.push_bind(if hid { 1i64 } else { 0i64 });
    }

    qb.push(" WHERE id = ");
    qb.push_bind(id.0);

    let res = qb.build().execute(&db.write).await.map_err(Error::from)?;
    Ok(res.rows_affected() > 0)
}

pub async fn delete_channel(db: &Db, id: ChannelId) -> Result<bool> {
    let res = sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    Ok(res.rows_affected() > 0)
}

pub async fn auto_map_channels_to_epg(db: &Db) -> Result<u32> {
    let res = sqlx::query(
        r#"UPDATE channels
           SET epg_channel_id = (
               SELECT epg_channel_id FROM epg_programs
               WHERE epg_programs.epg_channel_id = channels.channel_number
                  OR LOWER(epg_programs.epg_channel_id) = LOWER(channels.name)
               LIMIT 1
           )
           WHERE epg_channel_id IS NULL"#,
    )
    .execute(&db.write)
    .await
    .map_err(Error::from)?;

    Ok(res.rows_affected() as u32)
}

// ---------------------------------------------------------------------------
// EPG Programs
// ---------------------------------------------------------------------------

pub async fn batch_upsert_epg_programs(db: &Db, programs: &[EpgProgramInsert]) -> Result<u32> {
    if programs.is_empty() {
        return Ok(0);
    }

    let mut count = 0;
    for chunk in programs.chunks(50) {
        let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
            r#"INSERT INTO epg_programs (
                   channel_id, epg_channel_id, title, description,
                   season_number, episode_number, start_time, end_time,
                   icon_url, rating, genres, is_new
               ) "#,
        );

        qb.push_values(chunk, |mut b, p| {
            let genres_json = serde_json::to_string(&p.genres).unwrap_or_else(|_| "[]".into());
            b.push_bind(p.channel_id.map(|c| c.0))
                .push_bind(&p.epg_channel_id)
                .push_bind(&p.title)
                .push_bind(&p.description)
                .push_bind(p.season_number.map(|s| s as i64))
                .push_bind(p.episode_number.map(|e| e as i64))
                .push_bind(p.start_time)
                .push_bind(p.end_time)
                .push_bind(&p.icon_url)
                .push_bind(&p.rating)
                .push_bind(genres_json)
                .push_bind(if p.is_new { 1i64 } else { 0i64 });
        });

        let res = qb.build().execute(&db.write).await.map_err(Error::from)?;
        count += res.rows_affected();
    }

    // Link unassigned epg programs to matching channels by epg_channel_id
    let _ = sqlx::query(
        r#"UPDATE epg_programs
           SET channel_id = (
               SELECT id FROM channels
               WHERE channels.epg_channel_id = epg_programs.epg_channel_id
                  OR channels.channel_number = epg_programs.epg_channel_id
               LIMIT 1
           )
           WHERE channel_id IS NULL"#,
    )
    .execute(&db.write)
    .await;

    Ok(count as u32)
}

pub async fn get_programs_for_channel(
    db: &Db,
    channel_id: ChannelId,
    start_time: i64,
    end_time: i64,
) -> Result<Vec<EpgProgramRow>> {
    sqlx::query_as::<Sqlite, EpgProgramRow>(
        r#"SELECT id, channel_id, epg_channel_id, title, description,
                  season_number, episode_number, start_time, end_time,
                  icon_url, rating, genres, is_new
           FROM epg_programs
           WHERE (channel_id = ?1 OR (channel_id IS NULL AND epg_channel_id = (SELECT epg_channel_id FROM channels WHERE id = ?1)))
             AND end_time > ?2
             AND start_time < ?3
           ORDER BY start_time ASC"#,
    )
    .bind(channel_id.0)
    .bind(start_time)
    .bind(end_time)
    .fetch_all(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn get_current_and_next_program(
    db: &Db,
    channel_id: ChannelId,
    now: i64,
) -> Result<(Option<EpgProgramRow>, Option<EpgProgramRow>)> {
    let current = sqlx::query_as::<Sqlite, EpgProgramRow>(
        r#"SELECT id, channel_id, epg_channel_id, title, description,
                  season_number, episode_number, start_time, end_time,
                  icon_url, rating, genres, is_new
           FROM epg_programs
           WHERE channel_id = ?
             AND start_time <= ?
             AND end_time > ?
           ORDER BY start_time DESC
           LIMIT 1"#,
    )
    .bind(channel_id.0)
    .bind(now)
    .bind(now)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)?;

    let next = sqlx::query_as::<Sqlite, EpgProgramRow>(
        r#"SELECT id, channel_id, epg_channel_id, title, description,
                  season_number, episode_number, start_time, end_time,
                  icon_url, rating, genres, is_new
           FROM epg_programs
           WHERE channel_id = ?
             AND start_time > ?
           ORDER BY start_time ASC
           LIMIT 1"#,
    )
    .bind(channel_id.0)
    .bind(now)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)?;

    Ok((current, next))
}

pub async fn search_epg(
    db: &Db,
    query: &str,
    start_time: Option<i64>,
    end_time: Option<i64>,
    limit: u32,
) -> Result<Vec<EpgProgramRow>> {
    let pattern = format!("%{query}%");
    let start = start_time.unwrap_or(0);
    let end = end_time.unwrap_or(i64::MAX);

    sqlx::query_as::<Sqlite, EpgProgramRow>(
        r#"SELECT id, channel_id, epg_channel_id, title, description,
                  season_number, episode_number, start_time, end_time,
                  icon_url, rating, genres, is_new
           FROM epg_programs
           WHERE (title LIKE ? OR description LIKE ?)
             AND end_time > ?
             AND start_time < ?
           ORDER BY start_time ASC
           LIMIT ?"#,
    )
    .bind(&pattern)
    .bind(&pattern)
    .bind(start)
    .bind(end)
    .bind(limit as i64)
    .fetch_all(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn prune_expired_epg(db: &Db, before_ms: i64) -> Result<u64> {
    let res = sqlx::query("DELETE FROM epg_programs WHERE end_time < ?")
        .bind(before_ms)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    Ok(res.rows_affected())
}

// ---------------------------------------------------------------------------
// Recording Rules
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn create_recording_rule(
    db: &Db,
    user_id: UserId,
    title: &str,
    channel_id: Option<ChannelId>,
    series_only: bool,
    season_number: Option<u32>,
    new_only: bool,
    start_padding_secs: u32,
    end_padding_secs: u32,
    keep_max: Option<u32>,
    auto_delete_watched: bool,
) -> Result<RecordingRuleId> {
    let now = now_ms();
    let res = sqlx::query(
        r#"INSERT INTO recording_rules (
               user_id, title, channel_id, series_only, season_number,
               new_only, start_padding_secs, end_padding_secs, keep_max,
               auto_delete_watched, created_at
           )
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           RETURNING id"#,
    )
    .bind(user_id.0)
    .bind(title)
    .bind(channel_id.map(|c| c.0))
    .bind(if series_only { 1i64 } else { 0i64 })
    .bind(season_number.map(|s| s as i64))
    .bind(if new_only { 1i64 } else { 0i64 })
    .bind(start_padding_secs as i64)
    .bind(end_padding_secs as i64)
    .bind(keep_max.map(|k| k as i64))
    .bind(if auto_delete_watched { 1i64 } else { 0i64 })
    .bind(now)
    .fetch_one(&db.write)
    .await
    .map_err(Error::from)?;

    use sqlx::Row;
    let id: i64 = res.get(0);
    Ok(RecordingRuleId(id))
}

pub async fn list_recording_rules(
    db: &Db,
    user_id: Option<UserId>,
) -> Result<Vec<RecordingRuleRow>> {
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        r#"SELECT id, user_id, title, channel_id, series_only, season_number,
                  new_only, start_padding_secs, end_padding_secs, keep_max,
                  auto_delete_watched, created_at
           FROM recording_rules "#,
    );
    if let Some(uid) = user_id {
        qb.push("WHERE user_id = ");
        qb.push_bind(uid.0);
    }
    qb.push(" ORDER BY created_at DESC");
    qb.build_query_as::<RecordingRuleRow>().fetch_all(&db.read).await.map_err(Error::from)
}

pub async fn delete_recording_rule(db: &Db, id: RecordingRuleId) -> Result<bool> {
    let res = sqlx::query("DELETE FROM recording_rules WHERE id = ?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    Ok(res.rows_affected() > 0)
}

// ---------------------------------------------------------------------------
// Recordings
// ---------------------------------------------------------------------------

pub async fn schedule_recording(
    db: &Db,
    rule_id: Option<RecordingRuleId>,
    channel_id: ChannelId,
    program_id: Option<ProgramId>,
    title: &str,
    start_time: i64,
    end_time: i64,
) -> Result<RecordingId> {
    let now = now_ms();
    let res = sqlx::query(
        r#"INSERT INTO recordings (
               rule_id, channel_id, program_id, title, start_time,
               end_time, status, created_at
           )
           VALUES (?, ?, ?, ?, ?, ?, 'scheduled', ?)
           RETURNING id"#,
    )
    .bind(rule_id.map(|r| r.0))
    .bind(channel_id.0)
    .bind(program_id.map(|p| p.0))
    .bind(title)
    .bind(start_time)
    .bind(end_time)
    .bind(now)
    .fetch_one(&db.write)
    .await
    .map_err(Error::from)?;

    use sqlx::Row;
    let id: i64 = res.get(0);
    Ok(RecordingId(id))
}

pub async fn list_recordings(db: &Db, status: Option<&str>) -> Result<Vec<RecordingRow>> {
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        r#"SELECT r.id, r.rule_id, r.channel_id, c.name as channel_name,
                  r.program_id, r.title, r.start_time, r.end_time,
                  r.status, r.output_path, r.size_bytes, r.duration_ms,
                  r.error_message, r.created_at
           FROM recordings r
           LEFT JOIN channels c ON c.id = r.channel_id "#,
    );
    if let Some(st) = status {
        qb.push("WHERE r.status = ");
        qb.push_bind(st);
    }
    qb.push(" ORDER BY r.start_time DESC");
    qb.build_query_as::<RecordingRow>().fetch_all(&db.read).await.map_err(Error::from)
}

pub async fn get_recording(db: &Db, id: RecordingId) -> Result<Option<RecordingRow>> {
    sqlx::query_as::<Sqlite, RecordingRow>(
        r#"SELECT r.id, r.rule_id, r.channel_id, c.name as channel_name,
                  r.program_id, r.title, r.start_time, r.end_time,
                  r.status, r.output_path, r.size_bytes, r.duration_ms,
                  r.error_message, r.created_at
           FROM recordings r
           LEFT JOIN channels c ON c.id = r.channel_id
           WHERE r.id = ?"#,
    )
    .bind(id.0)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn update_recording_status(
    db: &Db,
    id: RecordingId,
    status: &str,
    output_path: Option<&str>,
    size_bytes: Option<u64>,
    duration_ms: Option<u64>,
    error_message: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE recordings
           SET status = ?, output_path = COALESCE(?, output_path),
               size_bytes = COALESCE(?, size_bytes), duration_ms = COALESCE(?, duration_ms),
               error_message = ?
           WHERE id = ?"#,
    )
    .bind(status)
    .bind(output_path)
    .bind(size_bytes.map(|s| s as i64))
    .bind(duration_ms.map(|d| d as i64))
    .bind(error_message)
    .bind(id.0)
    .execute(&db.write)
    .await
    .map_err(Error::from)?;

    Ok(())
}

pub async fn delete_recording(db: &Db, id: RecordingId) -> Result<bool> {
    let res = sqlx::query("DELETE FROM recordings WHERE id = ?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    Ok(res.rows_affected() > 0)
}

pub async fn count_overlapping_recordings(
    db: &Db,
    channel_id: ChannelId,
    start_time: i64,
    end_time: i64,
) -> Result<i64> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) as count
           FROM recordings
           WHERE channel_id = ?
             AND status IN ('scheduled', 'recording')
             AND end_time > ?
             AND start_time < ?"#,
    )
    .bind(channel_id.0)
    .bind(start_time)
    .bind(end_time)
    .fetch_one(&db.read)
    .await
    .map_err(Error::from)?;

    use sqlx::Row;
    let count: i64 = row.get(0);
    Ok(count)
}

// ---------------------------------------------------------------------------
// Virtual Channels
// ---------------------------------------------------------------------------

pub async fn create_virtual_channel(
    db: &Db,
    channel_id: ChannelId,
    name: &str,
    mode: &str,
    items: &[VirtualChannelItem],
    insert_commercials: bool,
) -> Result<VirtualChannelId> {
    let now = now_ms();
    let items_json = serde_json::to_string(items).unwrap_or_else(|_| "[]".into());
    let total_duration_ms: u64 = items.iter().map(|i| i.duration_ms).sum();

    let res = sqlx::query(
        r#"INSERT INTO virtual_channels (
               channel_id, name, mode, items, insert_commercials,
               total_duration_ms, created_at
           )
           VALUES (?, ?, ?, ?, ?, ?, ?)
           RETURNING id"#,
    )
    .bind(channel_id.0)
    .bind(name)
    .bind(mode)
    .bind(items_json)
    .bind(if insert_commercials { 1i64 } else { 0i64 })
    .bind(total_duration_ms as i64)
    .bind(now)
    .fetch_one(&db.write)
    .await
    .map_err(Error::from)?;

    use sqlx::Row;
    let id: i64 = res.get(0);
    Ok(VirtualChannelId(id))
}

pub async fn get_virtual_channel(
    db: &Db,
    id: VirtualChannelId,
) -> Result<Option<VirtualChannelRow>> {
    sqlx::query_as::<Sqlite, VirtualChannelRow>(
        r#"SELECT id, channel_id, name, mode, items, insert_commercials,
                  total_duration_ms, created_at
           FROM virtual_channels
           WHERE id = ?"#,
    )
    .bind(id.0)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn get_virtual_channel_by_channel_id(
    db: &Db,
    channel_id: ChannelId,
) -> Result<Option<VirtualChannelRow>> {
    sqlx::query_as::<Sqlite, VirtualChannelRow>(
        r#"SELECT id, channel_id, name, mode, items, insert_commercials,
                  total_duration_ms, created_at
           FROM virtual_channels
           WHERE channel_id = ?"#,
    )
    .bind(channel_id.0)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn list_virtual_channels(db: &Db) -> Result<Vec<VirtualChannelRow>> {
    sqlx::query_as::<Sqlite, VirtualChannelRow>(
        r#"SELECT id, channel_id, name, mode, items, insert_commercials,
                  total_duration_ms, created_at
           FROM virtual_channels
           ORDER BY name ASC"#,
    )
    .fetch_all(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn delete_virtual_channel(db: &Db, id: VirtualChannelId) -> Result<bool> {
    let res = sqlx::query("DELETE FROM virtual_channels WHERE id = ?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .map_err(Error::from)?;

    Ok(res.rows_affected() > 0)
}

pub async fn get_item_media_info(
    db: &Db,
    item_id: ItemId,
) -> Result<Option<(String, Option<i64>, Option<String>)>> {
    let row: Option<(String, Option<i64>, Option<String>)> = sqlx::query_as(
        r#"SELECT mi.title, mp.duration_ms, mp.path
           FROM media_items mi
           LEFT JOIN media_versions mv ON mv.item_id = mi.id
           LEFT JOIN media_parts mp ON mp.version_id = mv.id
           WHERE mi.id = ?
           LIMIT 1"#,
    )
    .bind(item_id.0)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)?;

    Ok(row)
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

pub async fn upsert_plugin_config(
    db: &Db,
    plugin_id: &str,
    name: &str,
    enabled: bool,
    settings: &serde_json::Value,
) -> Result<PluginId> {
    let now = now_ms();
    let settings_str = settings.to_string();

    let res = sqlx::query(
        r#"INSERT INTO plugin_configs (plugin_id, name, enabled, settings, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(plugin_id) DO UPDATE SET
               name = excluded.name,
               enabled = excluded.enabled,
               settings = excluded.settings,
               updated_at = excluded.updated_at
           RETURNING id"#,
    )
    .bind(plugin_id)
    .bind(name)
    .bind(if enabled { 1i64 } else { 0i64 })
    .bind(settings_str)
    .bind(now)
    .bind(now)
    .fetch_one(&db.write)
    .await
    .map_err(Error::from)?;

    use sqlx::Row;
    let id: i64 = res.get(0);
    Ok(PluginId(id))
}

pub async fn list_plugin_configs(db: &Db) -> Result<Vec<PluginConfigRow>> {
    sqlx::query_as::<Sqlite, PluginConfigRow>(
        r#"SELECT id, plugin_id, name, enabled, settings, created_at, updated_at
           FROM plugin_configs
           ORDER BY name ASC"#,
    )
    .fetch_all(&db.read)
    .await
    .map_err(Error::from)
}

pub async fn get_plugin_config(db: &Db, plugin_id: &str) -> Result<Option<PluginConfigRow>> {
    sqlx::query_as::<Sqlite, PluginConfigRow>(
        r#"SELECT id, plugin_id, name, enabled, settings, created_at, updated_at
           FROM plugin_configs
           WHERE plugin_id = ?"#,
    )
    .bind(plugin_id)
    .fetch_optional(&db.read)
    .await
    .map_err(Error::from)
}
