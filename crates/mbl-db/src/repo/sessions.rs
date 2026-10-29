use mbl_core::{Result, SessionId, UserId};
use serde::Serialize;

use crate::Db;

/// A session row joined with the owning user's role.
#[derive(Debug, Clone)]
pub struct SessionAuth {
    pub id: SessionId,
    pub user_id: UserId,
    pub is_admin: bool,
    pub last_seen_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewSession<'a> {
    pub user_id: UserId,
    pub token_hash: &'a [u8],
    pub device_name: Option<&'a str>,
    pub client_name: Option<&'a str>,
    pub now: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct SessionsRepo {
    db: Db,
}

impl SessionsRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Capability owners must still have the same, unexpired login. Session ids
    /// alone are insufficient because SQLite may reuse a deleted integer id.
    pub async fn active_identity(
        &self,
        id: SessionId,
        now: i64,
    ) -> Result<Option<(UserId, Vec<u8>)>> {
        Ok(sqlx::query_as("SELECT user_id,token_hash FROM sessions WHERE id=? AND expires_at>?")
            .bind(id.0)
            .bind(now)
            .fetch_optional(&self.db.read)
            .await?)
    }

    pub async fn insert(&self, s: &NewSession<'_>) -> Result<SessionId> {
        Ok(sqlx::query_scalar!(
            r#"INSERT INTO sessions
                 (user_id, token_hash, device_name, client_name, created_at, last_seen_at, expires_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               RETURNING id AS "id!: SessionId""#,
            s.user_id,
            s.token_hash,
            s.device_name,
            s.client_name,
            s.now,
            s.now,
            s.expires_at
        )
        .fetch_one(&self.db.write)
        .await?)
    }

    pub async fn find_by_token_hash(&self, token_hash: &[u8]) -> Result<Option<SessionAuth>> {
        Ok(sqlx::query_as!(
            SessionAuth,
            r#"SELECT s.id AS "id: SessionId", s.user_id AS "user_id: UserId",
                      u.is_admin AS "is_admin: bool", s.last_seen_at, s.expires_at
               FROM sessions s JOIN users u ON u.id = s.user_id
               WHERE s.token_hash = ?"#,
            token_hash
        )
        .fetch_optional(&self.db.read)
        .await?)
    }

    pub async fn touch(&self, id: SessionId, now: i64, expires_at: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE id = ?",
            now,
            expires_at,
            id
        )
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    pub async fn list_for_user(&self, user_id: UserId) -> Result<Vec<SessionInfo>> {
        Ok(sqlx::query_as!(
            SessionInfo,
            r#"SELECT id AS "id: SessionId", device_name, client_name,
                      created_at, last_seen_at, expires_at
               FROM sessions WHERE user_id = ? ORDER BY last_seen_at DESC"#,
            user_id
        )
        .fetch_all(&self.db.read)
        .await?)
    }

    pub async fn delete(&self, id: SessionId) -> Result<bool> {
        let res =
            sqlx::query!("DELETE FROM sessions WHERE id = ?", id).execute(&self.db.write).await?;
        Ok(res.rows_affected() > 0)
    }

    /// Delete a session only if it belongs to `user_id`.
    pub async fn delete_for_user(&self, user_id: UserId, id: SessionId) -> Result<bool> {
        let res = sqlx::query!("DELETE FROM sessions WHERE id = ? AND user_id = ?", id, user_id)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn delete_all_for_user(&self, user_id: UserId) -> Result<u64> {
        let res = sqlx::query!("DELETE FROM sessions WHERE user_id = ?", user_id)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected())
    }

    pub async fn delete_expired(&self, now: i64) -> Result<u64> {
        let res = sqlx::query!("DELETE FROM sessions WHERE expires_at <= ?", now)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected())
    }
}
