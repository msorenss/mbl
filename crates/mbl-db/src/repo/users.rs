use mbl_core::{Result, UserId};
use serde::Serialize;

use crate::{Db, map_unique};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub display_name: String,
    pub is_admin: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewUser<'a> {
    pub username: &'a str,
    pub display_name: &'a str,
    pub password_hash: &'a str,
    pub is_admin: bool,
}

#[derive(Debug, Clone)]
pub struct UsersRepo {
    db: Db,
}

impl UsersRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar!("SELECT COUNT(*) FROM users").fetch_one(&self.db.read).await?)
    }

    pub async fn count_admins(&self) -> Result<i64> {
        Ok(sqlx::query_scalar!("SELECT COUNT(*) FROM users WHERE is_admin = 1")
            .fetch_one(&self.db.read)
            .await?)
    }

    /// Atomically insert the first (admin) user. Returns `None` if any user already exists.
    pub async fn insert_first_admin(&self, u: &NewUser<'_>, now: i64) -> Result<Option<UserId>> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO users (username, display_name, password_hash, is_admin, created_at)
               SELECT ?, ?, ?, 1, ? WHERE NOT EXISTS (SELECT 1 FROM users)
               RETURNING id AS "id!: UserId""#,
            u.username,
            u.display_name,
            u.password_hash,
            now
        )
        .fetch_optional(&self.db.write)
        .await
        .map_err(|e| map_unique(e, "username already exists"))?;
        Ok(id)
    }

    pub async fn insert(&self, u: &NewUser<'_>, now: i64) -> Result<UserId> {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO users (username, display_name, password_hash, is_admin, created_at)
               VALUES (?, ?, ?, ?, ?)
               RETURNING id AS "id!: UserId""#,
            u.username,
            u.display_name,
            u.password_hash,
            u.is_admin,
            now
        )
        .fetch_one(&self.db.write)
        .await
        .map_err(|e| map_unique(e, "username already exists"))?;
        Ok(id)
    }

    pub async fn get(&self, id: UserId) -> Result<Option<User>> {
        Ok(sqlx::query_as!(
            User,
            r#"SELECT id AS "id: UserId", username, display_name,
                      is_admin AS "is_admin: bool", created_at
               FROM users WHERE id = ?"#,
            id
        )
        .fetch_optional(&self.db.read)
        .await?)
    }

    /// User plus password hash, for login.
    pub async fn get_credentials(&self, username: &str) -> Result<Option<(User, String)>> {
        let row = sqlx::query!(
            r#"SELECT id AS "id: UserId", username, display_name,
                      is_admin AS "is_admin: bool", created_at, password_hash
               FROM users WHERE username = ?"#,
            username
        )
        .fetch_optional(&self.db.read)
        .await?;
        Ok(row.map(|r| {
            let user = User {
                id: r.id,
                username: r.username,
                display_name: r.display_name,
                is_admin: r.is_admin,
                created_at: r.created_at,
            };
            (user, r.password_hash)
        }))
    }

    pub async fn list(&self) -> Result<Vec<User>> {
        Ok(sqlx::query_as!(
            User,
            r#"SELECT id AS "id: UserId", username, display_name,
                      is_admin AS "is_admin: bool", created_at
               FROM users ORDER BY username COLLATE NOCASE"#
        )
        .fetch_all(&self.db.read)
        .await?)
    }

    pub async fn update_display_name(&self, id: UserId, display_name: &str) -> Result<bool> {
        let res = sqlx::query!("UPDATE users SET display_name = ? WHERE id = ?", display_name, id)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn update_password(&self, id: UserId, password_hash: &str) -> Result<bool> {
        let res =
            sqlx::query!("UPDATE users SET password_hash = ? WHERE id = ?", password_hash, id)
                .execute(&self.db.write)
                .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn update_admin(&self, id: UserId, is_admin: bool) -> Result<bool> {
        let res = sqlx::query!("UPDATE users SET is_admin = ? WHERE id = ?", is_admin, id)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn delete(&self, id: UserId) -> Result<bool> {
        let res =
            sqlx::query!("DELETE FROM users WHERE id = ?", id).execute(&self.db.write).await?;
        Ok(res.rows_affected() > 0)
    }
}
