//! Repository for `hw_capabilities` functional self-test results (M4.3).

use mbl_core::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

use crate::Db;

/// Stored row in `hw_capabilities`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HwCapabilityRow {
    pub backend: String,
    pub codec: String,
    pub operation: String,
    pub ok: bool,
    pub fps: Option<f64>,
    pub error: Option<String>,
    pub ffmpeg_hash: String,
    pub driver_fingerprint: String,
    pub tested_at: i64,
}

/// Upsert a batch of self-test capability entries into `hw_capabilities`.
pub async fn upsert_all(db: &Db, entries: &[HwCapabilityRow]) -> Result<()> {
    let mut tx = db.write.begin().await?;
    for entry in entries {
        sqlx::query(
            r#"
            INSERT INTO hw_capabilities (
                backend, codec, operation, ok, fps, error, ffmpeg_hash, driver_fingerprint, tested_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(backend, codec, operation) DO UPDATE SET
                ok = excluded.ok,
                fps = excluded.fps,
                error = excluded.error,
                ffmpeg_hash = excluded.ffmpeg_hash,
                driver_fingerprint = excluded.driver_fingerprint,
                tested_at = excluded.tested_at
            "#,
        )
        .bind(&entry.backend)
        .bind(&entry.codec)
        .bind(&entry.operation)
        .bind(i64::from(entry.ok))
        .bind(entry.fps)
        .bind(&entry.error)
        .bind(&entry.ffmpeg_hash)
        .bind(&entry.driver_fingerprint)
        .bind(entry.tested_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// List all recorded hardware capability self-test rows ordered by backend, operation, and codec.
pub async fn list_all(db: &Db) -> Result<Vec<HwCapabilityRow>> {
    let rows = sqlx::query(
        r#"
        SELECT backend, codec, operation, ok, fps, error, ffmpeg_hash, driver_fingerprint, tested_at
        FROM hw_capabilities
        ORDER BY backend ASC, operation ASC, codec ASC
        "#,
    )
    .fetch_all(&db.read)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(HwCapabilityRow {
            backend: row.try_get("backend")?,
            codec: row.try_get("codec")?,
            operation: row.try_get("operation")?,
            ok: row.try_get::<i64, _>("ok")? != 0,
            fps: row.try_get("fps")?,
            error: row.try_get("error")?,
            ffmpeg_hash: row.try_get("ffmpeg_hash")?,
            driver_fingerprint: row.try_get("driver_fingerprint")?,
            tested_at: row.try_get("tested_at")?,
        });
    }
    Ok(out)
}

/// Check whether stored self-test rows are stale relative to the current FFmpeg hash and driver fingerprint.
pub async fn needs_retest(db: &Db, ffmpeg_hash: &str, driver_fingerprint: &str) -> Result<bool> {
    let rows = list_all(db).await?;
    if rows.is_empty() {
        return Ok(true);
    }
    Ok(rows
        .iter()
        .any(|r| r.ffmpeg_hash != ffmpeg_hash || r.driver_fingerprint != driver_fingerprint))
}
