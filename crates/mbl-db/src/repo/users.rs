use mbl_core::{Result, UserId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{Db, map_unique};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleStylePreferences {
    /// `"sm" | "md" | "lg" | "xl"`, default `"md"`.
    pub size: String,
    /// `"none" | "dim" | "solid"`, default `"dim"`.
    pub background: String,
    /// `"white" | "yellow" | "cyan"`, default `"white"`.
    pub color: String,
}

impl Default for SubtitleStylePreferences {
    fn default() -> Self {
        Self { size: "md".to_owned(), background: "dim".to_owned(), color: "white".to_owned() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct UserPreferences {
    /// `"dark" | "oled" | "light" | "high-contrast"`, default `"dark"`.
    pub theme: String,
    /// `"amber" | "indigo" | "emerald" | "rose" | "cyan" | "violet"`, default `"amber"`.
    pub accent: String,
    /// `"en" | "sv" | "de"`, default `"en"`.
    pub locale: String,
    pub reduced_motion: bool,
    pub night_mode: bool,
    pub audio_offset_ms: i64,
    pub subtitle_style: SubtitleStylePreferences,
    /// Ordered hub keys (`"continueWatching"`, `"upNext"`, `"recentlyAdded.<id>"`).
    pub home_layout: Vec<String>,
    /// Hub keys hidden by the user on the Home screen.
    pub hidden_hubs: Vec<String>,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            theme: "dark".to_owned(),
            accent: "amber".to_owned(),
            locale: "en".to_owned(),
            reduced_motion: false,
            night_mode: false,
            audio_offset_ms: 0,
            subtitle_style: SubtitleStylePreferences::default(),
            home_layout: Vec::new(),
            hidden_hubs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserPreferences {
    pub theme: Option<String>,
    pub accent: Option<String>,
    pub locale: Option<String>,
    pub reduced_motion: Option<bool>,
    pub night_mode: Option<bool>,
    pub audio_offset_ms: Option<i64>,
    pub subtitle_style: Option<SubtitleStylePreferences>,
    pub home_layout: Option<Vec<String>>,
    pub hidden_hubs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HomeLayoutRequest {
    pub home_layout: Vec<String>,
    #[serde(default)]
    pub hidden_hubs: Option<Vec<String>>,
}

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

    pub async fn get_preferences(&self, id: UserId) -> Result<Option<UserPreferences>> {
        let raw: Option<String> = sqlx::query_scalar("SELECT preferences FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db.read)
            .await?;
        Ok(raw.map(|s| serde_json::from_str::<UserPreferences>(&s).unwrap_or_default()))
    }

    pub async fn set_preferences(&self, id: UserId, prefs: &UserPreferences) -> Result<bool> {
        let json = serde_json::to_string(prefs).map_err(mbl_core::Error::internal)?;
        let res = sqlx::query("UPDATE users SET preferences = ? WHERE id = ?")
            .bind(json)
            .bind(id)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected() > 0)
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
