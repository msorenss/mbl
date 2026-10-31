use mbl_core::online::OnlineSource;
use mbl_core::{Error, Result, UserId};

use crate::Db;

#[derive(Debug, Clone)]
pub struct OnlineRepo {
    db: Db,
}

impl OnlineRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn get(&self, owner: UserId, id: &str) -> Result<Option<OnlineSource>> {
        let document: Option<String> =
            sqlx::query_scalar("SELECT document FROM online_sources WHERE owner_id=? AND id=?")
                .bind(owner)
                .bind(id)
                .fetch_optional(&self.db.read)
                .await?;
        document.map(|s| serde_json::from_str(&s).map_err(Error::internal)).transpose()
    }

    pub async fn recent(&self, owner: UserId) -> Result<Vec<OnlineSource>> {
        let documents: Vec<String> = sqlx::query_scalar(
            "SELECT document FROM online_sources WHERE owner_id=? ORDER BY inspected_at DESC, id LIMIT 200",
        )
        .bind(owner)
        .fetch_all(&self.db.read)
        .await?;
        documents.iter().map(|s| serde_json::from_str(s).map_err(Error::internal)).collect()
    }

    pub async fn save(&self, owner: UserId, source: &OnlineSource) -> Result<()> {
        let document = serde_json::to_string(source).map_err(Error::internal)?;
        if document.len() > 64 * 1024 {
            return Err(Error::Validation("Source metadata exceeds its limit".into()));
        }
        let mut tx = self.db.write.begin().await?;
        sqlx::query("INSERT INTO online_sources(owner_id,id,url,document,inspected_at) VALUES(?,?,?,?,?) ON CONFLICT(owner_id,id) DO UPDATE SET url=excluded.url,document=excluded.document,inspected_at=excluded.inspected_at")
            .bind(owner).bind(&source.id).bind(&source.url).bind(document).bind(source.checked_at.unwrap_or_default()).execute(&mut *tx).await?;
        // Bound the derived cache globally; it contains no saved library or watch state.
        sqlx::query("DELETE FROM online_sources WHERE (owner_id,id) IN (SELECT owner_id,id FROM online_sources ORDER BY inspected_at DESC,owner_id,id LIMIT -1 OFFSET 512)")
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

impl OnlineRepo {
    pub async fn position(&self, owner: UserId, source: &OnlineSource) -> Result<f64> {
        let row: Option<(String, f64)> = sqlx::query_as(
            "SELECT document,position FROM online_watch WHERE owner_id=? AND source_id=?",
        )
        .bind(owner)
        .bind(&source.id)
        .fetch_optional(&self.db.read)
        .await?;
        let Some((document, position)) = row else {
            return Ok(0.0);
        };
        let old: OnlineSource = serde_json::from_str(&document).map_err(Error::internal)?;
        if old.title == source.title
            && old.series_title == source.series_title
            && old.season_number == source.season_number
            && old.episode_number == source.episode_number
        {
            Ok(position)
        } else {
            Ok(0.0)
        }
    }
    pub async fn progress(
        &self,
        owner: UserId,
        source: &OnlineSource,
        position: f64,
        duration: f64,
    ) -> Result<()> {
        let document = serde_json::to_string(source).map_err(Error::internal)?;
        if document.len() > 64 * 1024 {
            return Err(Error::Validation("Online history metadata exceeds limit".into()));
        }
        let mut tx = self.db.write.begin().await?;
        sqlx::query("INSERT INTO online_watch(owner_id,source_id,document,position,duration,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(owner_id,source_id) DO UPDATE SET document=excluded.document,position=excluded.position,duration=excluded.duration,updated_at=excluded.updated_at")
            .bind(owner).bind(&source.id).bind(document).bind(position).bind(duration).bind(mbl_core::time::now_ms()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM online_watch WHERE owner_id=? AND source_id IN (SELECT source_id FROM online_watch WHERE owner_id=? ORDER BY updated_at DESC,source_id LIMIT -1 OFFSET 200)")
            .bind(owner).bind(owner).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

impl OnlineRepo {
    pub async fn history(
        &self,
        owner: UserId,
    ) -> Result<Vec<mbl_core::online::OnlineHistoryEntry>> {
        let rows: Vec<(String,f64,f64,i64)> = sqlx::query_as("SELECT document,position,duration,updated_at FROM online_watch WHERE owner_id=? ORDER BY updated_at DESC,source_id LIMIT 20")
            .bind(owner).fetch_all(&self.db.read).await?;
        rows.into_iter()
            .map(|(document, position, duration, updated_at)| {
                Ok(mbl_core::online::OnlineHistoryEntry {
                    source: serde_json::from_str(&document).map_err(Error::internal)?,
                    position,
                    duration,
                    updated_at,
                })
            })
            .collect()
    }
}
