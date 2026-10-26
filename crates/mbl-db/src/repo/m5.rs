//! M5 repositories: markers (intro/credits), trickplay, optimized versions, quick-connect, and scene embeddings.

use mbl_core::{ItemId, LibraryId, PartId, Result, UserId, VersionId};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::Db;

fn db_err(e: sqlx::Error) -> mbl_core::Error {
    e.into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarkerRow {
    pub id: i64,
    pub item_id: ItemId,
    pub part_id: Option<PartId>,
    pub kind: String,
    pub title: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub source: String,
    pub confidence: f64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrickplayRow {
    pub part_id: PartId,
    pub interval_ms: i64,
    pub tile_width: i64,
    pub tile_height: i64,
    pub columns: i64,
    pub rows: i64,
    pub frame_count: i64,
    pub sprite_count: i64,
    pub manifest_json: String,
    pub generated_ms: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationRuleRow {
    pub id: i64,
    pub library_id: Option<LibraryId>,
    pub item_id: Option<ItemId>,
    pub profile: String,
    pub night_window_only: bool,
    pub status: String,
    pub version_id: Option<VersionId>,
    pub output_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickConnectRow {
    pub code: String,
    pub secret: String,
    pub device_name: String,
    pub client_name: String,
    pub user_id: Option<UserId>,
    pub session_token: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SceneEmbeddingRow {
    pub id: i64,
    pub item_id: ItemId,
    pub part_id: Option<PartId>,
    pub timestamp_ms: i64,
    pub tags: Vec<String>,
    pub caption: String,
    pub embedding: Vec<f32>,
    pub backend: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct MarkersRepo {
    db: Db,
}

impl MarkersRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list_for_item(&self, item_id: ItemId) -> Result<Vec<MarkerRow>> {
        let rows = sqlx::query(
            "SELECT id, item_id, part_id, kind, title, start_ms, end_ms, source, confidence, created_at
             FROM markers WHERE item_id = ? ORDER BY start_ms ASC",
        )
        .bind(item_id.0)
        .fetch_all(&self.db.read)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| MarkerRow {
                id: r.get("id"),
                item_id: ItemId(r.get("item_id")),
                part_id: r.get::<Option<i64>, _>("part_id").map(PartId),
                kind: r.get("kind"),
                title: r.get("title"),
                start_ms: r.get("start_ms"),
                end_ms: r.get("end_ms"),
                source: r.get("source"),
                confidence: r.get("confidence"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        &self,
        item_id: ItemId,
        part_id: Option<PartId>,
        kind: &str,
        title: Option<&str>,
        start_ms: i64,
        end_ms: i64,
        source: &str,
        confidence: f64,
        now: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO markers (item_id, part_id, kind, title, start_ms, end_ms, source, confidence, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(item_id, kind, start_ms) DO UPDATE SET
               part_id = excluded.part_id,
               title = excluded.title,
               end_ms = excluded.end_ms,
               source = excluded.source,
               confidence = excluded.confidence",
        )
        .bind(item_id.0)
        .bind(part_id.map(|p| p.0))
        .bind(kind)
        .bind(title)
        .bind(start_ms)
        .bind(end_ms)
        .bind(source)
        .bind(confidence)
        .bind(now)
        .execute(&self.db.write)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn replace_for_item_kind(
        &self,
        item_id: ItemId,
        part_id: Option<PartId>,
        kind: &str,
        title: Option<&str>,
        start_ms: i64,
        end_ms: i64,
        source: &str,
        confidence: f64,
        now: i64,
    ) -> Result<()> {
        sqlx::query("DELETE FROM markers WHERE item_id = ? AND kind = ?")
            .bind(item_id.0)
            .bind(kind)
            .execute(&self.db.write)
            .await
            .map_err(db_err)?;
        self.upsert(item_id, part_id, kind, title, start_ms, end_ms, source, confidence, now).await
    }
}

#[derive(Debug, Clone)]
pub struct TrickplayRepo {
    db: Db,
}

impl TrickplayRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn get_for_part(&self, part_id: PartId) -> Result<Option<TrickplayRow>> {
        let row = sqlx::query(
            "SELECT part_id, interval_ms, tile_width, tile_height, columns, rows,
                    frame_count, sprite_count, manifest_json, generated_ms, created_at
             FROM trickplay WHERE part_id = ?",
        )
        .bind(part_id.0)
        .fetch_optional(&self.db.read)
        .await
        .map_err(db_err)?;

        Ok(row.map(|r| TrickplayRow {
            part_id: PartId(r.get("part_id")),
            interval_ms: r.get("interval_ms"),
            tile_width: r.get("tile_width"),
            tile_height: r.get("tile_height"),
            columns: r.get("columns"),
            rows: r.get("rows"),
            frame_count: r.get("frame_count"),
            sprite_count: r.get("sprite_count"),
            manifest_json: r.get("manifest_json"),
            generated_ms: r.get("generated_ms"),
            created_at: r.get("created_at"),
        }))
    }

    pub async fn upsert(&self, row: &TrickplayRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO trickplay (
                part_id, interval_ms, tile_width, tile_height, columns, rows,
                frame_count, sprite_count, manifest_json, generated_ms, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(part_id) DO UPDATE SET
                interval_ms = excluded.interval_ms,
                tile_width = excluded.tile_width,
                tile_height = excluded.tile_height,
                columns = excluded.columns,
                rows = excluded.rows,
                frame_count = excluded.frame_count,
                sprite_count = excluded.sprite_count,
                manifest_json = excluded.manifest_json,
                generated_ms = excluded.generated_ms,
                created_at = excluded.created_at",
        )
        .bind(row.part_id.0)
        .bind(row.interval_ms)
        .bind(row.tile_width)
        .bind(row.tile_height)
        .bind(row.columns)
        .bind(row.rows)
        .bind(row.frame_count)
        .bind(row.sprite_count)
        .bind(&row.manifest_json)
        .bind(row.generated_ms)
        .bind(row.created_at)
        .execute(&self.db.write)
        .await
        .map_err(db_err)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OptimizationRepo {
    db: Db,
}

impl OptimizationRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create_rule(
        &self,
        library_id: Option<LibraryId>,
        item_id: Option<ItemId>,
        profile: &str,
        night_window_only: bool,
        now: i64,
    ) -> Result<i64> {
        let res = sqlx::query(
            "INSERT INTO optimization_rules (library_id, item_id, profile, night_window_only, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'queued', ?, ?)",
        )
        .bind(library_id.map(|l| l.0))
        .bind(item_id.map(|i| i.0))
        .bind(profile)
        .bind(i64::from(night_window_only))
        .bind(now)
        .bind(now)
        .execute(&self.db.write)
        .await
        .map_err(db_err)?;
        Ok(res.last_insert_rowid())
    }

    pub async fn update_rule_completed(
        &self,
        rule_id: i64,
        version_id: VersionId,
        output_path: &str,
        now: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE optimization_rules SET status = 'completed', version_id = ?, output_path = ?, updated_at = ? WHERE id = ?",
        )
        .bind(version_id.0)
        .bind(output_path)
        .bind(now)
        .bind(rule_id)
        .execute(&self.db.write)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_for_item(&self, item_id: ItemId) -> Result<Vec<OptimizationRuleRow>> {
        let rows = sqlx::query(
            "SELECT id, library_id, item_id, profile, night_window_only, status, version_id, output_path, created_at, updated_at
             FROM optimization_rules WHERE item_id = ? ORDER BY id DESC",
        )
        .bind(item_id.0)
        .fetch_all(&self.db.read)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| OptimizationRuleRow {
                id: r.get("id"),
                library_id: r.get::<Option<i64>, _>("library_id").map(LibraryId),
                item_id: r.get::<Option<i64>, _>("item_id").map(ItemId),
                profile: r.get("profile"),
                night_window_only: r.get::<i64, _>("night_window_only") != 0,
                status: r.get("status"),
                version_id: r.get::<Option<i64>, _>("version_id").map(VersionId),
                output_path: r.get("output_path"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_optimized_version_and_part(
        &self,
        item_id: ItemId,
        profile_name: &str,
        opt_path_str: &str,
        size: i64,
        mtime: i64,
        duration_ms: i64,
        width: i64,
        height: i64,
        bitrate: i64,
        probe_json: &str,
    ) -> Result<(VersionId, PartId)> {
        use crate::repo::media::{
            NewPart, NewStream, VersionSummary, insert_part, insert_version, replace_streams,
        };

        let edition = format!("Optimized ({profile_name})");
        let mut tx = self.db.write.begin().await.map_err(db_err)?;
        let version_id = insert_version(
            &mut tx,
            item_id,
            &VersionSummary {
                edition: Some(&edition),
                container: Some("mp4"),
                bitrate: Some(bitrate),
                width: Some(width),
                height: Some(height),
                video_codec: Some("h264"),
                video_profile: Some("high"),
                bit_depth: Some(8),
                hdr: None,
                audio_codec: Some("aac"),
                audio_channels: Some(2),
            },
        )
        .await?;

        sqlx::query(
            "UPDATE media_versions SET is_optimized = 1, optimized_profile = ? WHERE id = ?",
        )
        .bind(profile_name)
        .bind(version_id.0)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        let part_id = insert_part(
            &mut tx,
            &NewPart {
                version_id,
                path: opt_path_str,
                size,
                mtime,
                quick_hash: None,
                duration_ms: Some(duration_ms),
                probe: probe_json,
                chapters: "[]",
                part_index: 0,
            },
        )
        .await?;

        replace_streams(
            &mut tx,
            part_id,
            &[
                NewStream {
                    idx: 0,
                    kind: "video",
                    codec: "h264",
                    profile: Some("High"),
                    language: Some("und"),
                    title: None,
                    is_default: true,
                    is_forced: false,
                    external_path: None,
                    width: Some(width),
                    height: Some(height),
                    bit_depth: Some(8),
                    hdr: None,
                    channels: None,
                    channel_layout: None,
                },
                NewStream {
                    idx: 1,
                    kind: "audio",
                    codec: "aac",
                    profile: Some("LC"),
                    language: Some("eng"),
                    title: Some("Stereo"),
                    is_default: true,
                    is_forced: false,
                    external_path: None,
                    width: None,
                    height: None,
                    bit_depth: None,
                    hdr: None,
                    channels: Some(2),
                    channel_layout: Some("stereo"),
                },
            ],
        )
        .await?;

        tx.commit().await.map_err(db_err)?;
        Ok((version_id, part_id))
    }
}

#[derive(Debug, Clone)]
pub struct QuickConnectRepo {
    db: Db,
}

impl QuickConnectRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn insert(
        &self,
        code: &str,
        secret: &str,
        device_name: &str,
        client_name: &str,
        now: i64,
        expires_at: i64,
    ) -> Result<()> {
        sqlx::query("DELETE FROM quick_connect_codes WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.db.write)
            .await
            .map_err(db_err)?;

        sqlx::query(
            "INSERT INTO quick_connect_codes (code, secret, device_name, client_name, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(code)
        .bind(secret)
        .bind(device_name)
        .bind(client_name)
        .bind(now)
        .bind(expires_at)
        .execute(&self.db.write)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn get_by_code(&self, code: &str, now: i64) -> Result<Option<QuickConnectRow>> {
        let row = sqlx::query(
            "SELECT code, secret, device_name, client_name, user_id, session_token, created_at, expires_at
             FROM quick_connect_codes WHERE code = ? AND expires_at > ?",
        )
        .bind(code)
        .bind(now)
        .fetch_optional(&self.db.read)
        .await
        .map_err(db_err)?;

        Ok(row.map(|r| QuickConnectRow {
            code: r.get("code"),
            secret: r.get("secret"),
            device_name: r.get("device_name"),
            client_name: r.get("client_name"),
            user_id: r.get::<Option<i64>, _>("user_id").map(UserId),
            session_token: r.get("session_token"),
            created_at: r.get("created_at"),
            expires_at: r.get("expires_at"),
        }))
    }

    pub async fn authorize(
        &self,
        code: &str,
        user_id: UserId,
        session_token: &str,
        now: i64,
    ) -> Result<bool> {
        let res = sqlx::query(
            "UPDATE quick_connect_codes
             SET user_id = ?, session_token = ?
             WHERE code = ? AND expires_at > ? AND user_id IS NULL",
        )
        .bind(user_id.0)
        .bind(session_token)
        .bind(code)
        .bind(now)
        .execute(&self.db.write)
        .await
        .map_err(db_err)?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn poll_by_secret(&self, secret: &str, now: i64) -> Result<Option<QuickConnectRow>> {
        let row = sqlx::query(
            "SELECT code, secret, device_name, client_name, user_id, session_token, created_at, expires_at
             FROM quick_connect_codes WHERE secret = ? AND expires_at > ?",
        )
        .bind(secret)
        .bind(now)
        .fetch_optional(&self.db.read)
        .await
        .map_err(db_err)?;

        let Some(r) = row else {
            return Ok(None);
        };
        let qc = QuickConnectRow {
            code: r.get("code"),
            secret: r.get("secret"),
            device_name: r.get("device_name"),
            client_name: r.get("client_name"),
            user_id: r.get::<Option<i64>, _>("user_id").map(UserId),
            session_token: r.get("session_token"),
            created_at: r.get("created_at"),
            expires_at: r.get("expires_at"),
        };

        // Once the TV client receives its session token, delete the one-time code row.
        if qc.session_token.is_some() {
            sqlx::query("DELETE FROM quick_connect_codes WHERE secret = ?")
                .bind(secret)
                .execute(&self.db.write)
                .await
                .map_err(db_err)?;
        }

        Ok(Some(qc))
    }
}

#[derive(Debug, Clone)]
pub struct SceneEmbeddingsRepo {
    db: Db,
}

impl SceneEmbeddingsRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        &self,
        item_id: ItemId,
        part_id: Option<PartId>,
        timestamp_ms: i64,
        tags: &[String],
        caption: &str,
        embedding: &[f32],
        backend: &str,
        now: i64,
    ) -> Result<()> {
        let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".into());
        let emb_json = serde_json::to_string(embedding).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            "INSERT INTO scene_embeddings (item_id, part_id, timestamp_ms, tags, caption, embedding_json, backend, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(item_id, timestamp_ms) DO UPDATE SET
                part_id = excluded.part_id,
                tags = excluded.tags,
                caption = excluded.caption,
                embedding_json = excluded.embedding_json,
                backend = excluded.backend,
                created_at = excluded.created_at",
        )
        .bind(item_id.0)
        .bind(part_id.map(|p| p.0))
        .bind(timestamp_ms)
        .bind(tags_json)
        .bind(caption)
        .bind(emb_json)
        .bind(backend)
        .bind(now)
        .execute(&self.db.write)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_for_item(&self, item_id: ItemId) -> Result<Vec<SceneEmbeddingRow>> {
        let rows = sqlx::query(
            "SELECT id, item_id, part_id, timestamp_ms, tags, caption, embedding_json, backend, created_at
             FROM scene_embeddings WHERE item_id = ? ORDER BY timestamp_ms ASC",
        )
        .bind(item_id.0)
        .fetch_all(&self.db.read)
        .await
        .map_err(db_err)?;

        Ok(rows.into_iter().map(parse_scene_row).collect())
    }

    pub async fn list_all(&self) -> Result<Vec<SceneEmbeddingRow>> {
        let rows = sqlx::query(
            "SELECT id, item_id, part_id, timestamp_ms, tags, caption, embedding_json, backend, created_at
             FROM scene_embeddings ORDER BY item_id ASC, timestamp_ms ASC",
        )
        .fetch_all(&self.db.read)
        .await
        .map_err(db_err)?;

        Ok(rows.into_iter().map(parse_scene_row).collect())
    }
}

fn parse_scene_row(r: sqlx::sqlite::SqliteRow) -> SceneEmbeddingRow {
    let tags_str: String = r.get("tags");
    let emb_str: String = r.get("embedding_json");
    SceneEmbeddingRow {
        id: r.get("id"),
        item_id: ItemId(r.get("item_id")),
        part_id: r.get::<Option<i64>, _>("part_id").map(PartId),
        timestamp_ms: r.get("timestamp_ms"),
        tags: serde_json::from_str(&tags_str).unwrap_or_default(),
        caption: r.get("caption"),
        embedding: serde_json::from_str(&emb_str).unwrap_or_default(),
        backend: r.get("backend"),
        created_at: r.get("created_at"),
    }
}
