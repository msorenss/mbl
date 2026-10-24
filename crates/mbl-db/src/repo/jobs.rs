use mbl_core::{JobId, Result};
use serde::Serialize;

use crate::Db;

pub const QUEUED: &str = "queued";
pub const RUNNING: &str = "running";
pub const DONE: &str = "done";
pub const FAILED: &str = "failed";
pub const CANCELLED: &str = "cancelled";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Job {
    pub id: JobId,
    pub kind: String,
    pub payload: String,
    pub status: String,
    pub priority: i64,
    pub progress: f64,
    pub attempts: i64,
    pub run_after: i64,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct JobsRepo {
    db: Db,
}

impl JobsRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn insert(
        &self,
        kind: &str,
        payload: &str,
        priority: i64,
        now: i64,
    ) -> Result<JobId> {
        Ok(sqlx::query_scalar!(
            r#"INSERT INTO jobs (kind, payload, status, priority, run_after, created_at, updated_at)
               VALUES (?, ?, 'queued', ?, ?, ?, ?)
               RETURNING id AS "id!: JobId""#,
            kind,
            payload,
            priority,
            now,
            now,
            now
        )
        .fetch_one(&self.db.write)
        .await?)
    }

    /// An existing queued/running job with the same kind and payload, if any.
    pub async fn find_active(&self, kind: &str, payload: &str) -> Result<Option<JobId>> {
        Ok(sqlx::query_scalar!(
            r#"SELECT id AS "id!: JobId" FROM jobs
               WHERE kind = ? AND payload = ? AND status IN ('queued', 'running')
               ORDER BY id LIMIT 1"#,
            kind,
            payload
        )
        .fetch_optional(&self.db.write)
        .await?)
    }

    /// Atomically claim the next runnable job among `kinds` (a JSON array of kind names) with
    /// `priority <= max_priority`, ordered by priority, then due time, then age.
    pub async fn claim_next(
        &self,
        kinds_json: &str,
        max_priority: i64,
        now: i64,
    ) -> Result<Option<Job>> {
        Ok(sqlx::query_as!(
            Job,
            r#"UPDATE jobs SET status = 'running', attempts = attempts + 1, updated_at = ?1
               WHERE id = (
                   SELECT id FROM jobs
                   WHERE status = 'queued' AND run_after <= ?1 AND priority <= ?2
                     AND kind IN (SELECT value FROM json_each(?3))
                   ORDER BY priority, run_after, id LIMIT 1)
               RETURNING id AS "id!: JobId", kind AS "kind!", payload AS "payload!",
                         status AS "status!", priority AS "priority!", progress AS "progress!",
                         attempts AS "attempts!", run_after AS "run_after!", error,
                         created_at AS "created_at!", updated_at AS "updated_at!""#,
            now,
            max_priority,
            kinds_json
        )
        .fetch_optional(&self.db.write)
        .await?)
    }

    pub async fn set_progress(&self, id: JobId, progress: f64, now: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE jobs SET progress = ?, updated_at = ? WHERE id = ?",
            progress,
            now,
            id
        )
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    pub async fn finish(
        &self,
        id: JobId,
        status: &str,
        error: Option<&str>,
        now: i64,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE jobs SET status = ?, error = ?, progress = CASE WHEN ? = 'done' THEN 1.0 ELSE progress END,
                    updated_at = ? WHERE id = ?",
            status,
            error,
            status,
            now,
            id
        )
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    pub async fn retry_later(
        &self,
        id: JobId,
        error: &str,
        run_after: i64,
        now: i64,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE jobs SET status = 'queued', error = ?, run_after = ?, updated_at = ? WHERE id = ?",
            error,
            run_after,
            now,
            id
        )
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    /// Re-queue jobs left `running` by a crash or unclean shutdown.
    pub async fn requeue_running(&self, now: i64) -> Result<u64> {
        let res = sqlx::query!(
            "UPDATE jobs SET status = 'queued', run_after = ?, updated_at = ? WHERE status = 'running'",
            now,
            now
        )
        .execute(&self.db.write)
        .await?;
        Ok(res.rows_affected())
    }

    /// Cancel a queued job directly. Returns false if it is not queued.
    pub async fn cancel_queued(&self, id: JobId, now: i64) -> Result<bool> {
        let res = sqlx::query!(
            "UPDATE jobs SET status = 'cancelled', updated_at = ? WHERE id = ? AND status = 'queued'",
            now,
            id
        )
        .execute(&self.db.write)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn get(&self, id: JobId) -> Result<Option<Job>> {
        Ok(sqlx::query_as!(
            Job,
            r#"SELECT id AS "id: JobId", kind, payload, status, priority, progress, attempts,
                      run_after, error, created_at, updated_at
               FROM jobs WHERE id = ?"#,
            id
        )
        .fetch_optional(&self.db.read)
        .await?)
    }

    /// Most recent jobs first.
    pub async fn list_recent(&self, limit: i64) -> Result<Vec<Job>> {
        Ok(sqlx::query_as!(
            Job,
            r#"SELECT id AS "id: JobId", kind, payload, status, priority, progress, attempts,
                      run_after, error, created_at, updated_at
               FROM jobs ORDER BY id DESC LIMIT ?"#,
            limit
        )
        .fetch_all(&self.db.read)
        .await?)
    }
}
