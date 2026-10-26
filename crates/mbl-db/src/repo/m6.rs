//! Milestone M6 database repositories:
//! - User library ACLs (`user_library_access`)
//! - Managed users, PIN hashes, content rating ceilings, TOTP secrets
//! - Sharing invites (`invites`)
//! - Remote access settings (`remote_settings`)
//! - OAuth 2.1 clients and grants (`oauth_clients`, `oauth_grants`)

use std::collections::HashMap;

use mbl_core::{ItemId, LibraryId, Result, UserId, time::now_ms};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

use crate::Db;

/// Security and access profile for a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserSecurityInfo {
    pub user_id: UserId,
    pub is_managed: bool,
    pub has_pin: bool,
    pub content_rating_ceiling: Option<String>,
    pub totp_enabled: bool,
    pub library_ids: Vec<LibraryId>,
}

/// Invitation to register with pre-assigned permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InviteRow {
    pub id: i64,
    pub code: String,
    pub created_by: UserId,
    pub library_ids: Vec<LibraryId>,
    pub content_rating_ceiling: Option<String>,
    pub expires_at: i64,
    pub used_at: Option<i64>,
    pub used_by: Option<UserId>,
    pub created_at: i64,
}

/// Dynamic OAuth 2.1 registered client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OAuthClient {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth: String,
    pub created_at: i64,
}

/// OAuth 2.1 grant and active token records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthGrant {
    pub id: i64,
    pub code: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub client_id: String,
    pub user_id: UserId,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    pub access_token_hash: Option<Vec<u8>>,
    pub refresh_token_hash: Option<Vec<u8>>,
    pub code_expires_at: Option<i64>,
    pub access_token_expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

// -----------------------------------------------------------------------------
// 1. User Library Access ACLs
// -----------------------------------------------------------------------------

/// Fetch list of library IDs explicitly granted to a user.
pub async fn get_user_libraries(db: &Db, user_id: UserId) -> Result<Vec<LibraryId>> {
    let rows = sqlx::query(
        "SELECT library_id FROM user_library_access WHERE user_id = ? ORDER BY library_id",
    )
    .bind(user_id.0)
    .fetch_all(&db.read)
    .await?;
    Ok(rows.into_iter().map(|r| LibraryId(r.get("library_id"))).collect())
}

/// Overwrite the library access list for a user.
pub async fn set_user_libraries(db: &Db, user_id: UserId, library_ids: &[LibraryId]) -> Result<()> {
    let mut tx = db.write.begin().await?;
    sqlx::query("DELETE FROM user_library_access WHERE user_id = ?")
        .bind(user_id.0)
        .execute(&mut *tx)
        .await?;

    let now = now_ms();
    for id in library_ids {
        sqlx::query(
            "INSERT INTO user_library_access (user_id, library_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(user_id.0)
        .bind(id.0)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Grant access to a single library.
pub async fn grant_library_access(db: &Db, user_id: UserId, library_id: LibraryId) -> Result<()> {
    let now = now_ms();
    sqlx::query(
        "INSERT OR IGNORE INTO user_library_access (user_id, library_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(user_id.0)
    .bind(library_id.0)
    .bind(now)
    .execute(&db.write)
    .await?;
    Ok(())
}

/// Revoke access to a single library.
pub async fn revoke_library_access(db: &Db, user_id: UserId, library_id: LibraryId) -> Result<()> {
    sqlx::query("DELETE FROM user_library_access WHERE user_id = ? AND library_id = ?")
        .bind(user_id.0)
        .bind(library_id.0)
        .execute(&db.write)
        .await?;
    Ok(())
}

// -----------------------------------------------------------------------------
// 2. User Security & Permissions (Managed, PIN, Content Rating, TOTP)
// -----------------------------------------------------------------------------

pub async fn get_user_security_info(db: &Db, user_id: UserId) -> Result<Option<UserSecurityInfo>> {
    let row = sqlx::query(
        "SELECT is_managed, pin_hash, content_rating_ceiling, totp_enabled FROM users WHERE id = ?",
    )
    .bind(user_id.0)
    .fetch_optional(&db.read)
    .await?;

    let Some(r) = row else { return Ok(None) };
    let is_managed: bool = r.get::<i64, _>("is_managed") != 0;
    let pin_hash: Option<String> = r.get("pin_hash");
    let content_rating_ceiling: Option<String> = r.get("content_rating_ceiling");
    let totp_enabled: bool = r.get::<i64, _>("totp_enabled") != 0;
    let library_ids = get_user_libraries(db, user_id).await?;

    Ok(Some(UserSecurityInfo {
        user_id,
        is_managed,
        has_pin: pin_hash.is_some(),
        content_rating_ceiling,
        totp_enabled,
        library_ids,
    }))
}

pub async fn update_user_permissions(
    db: &Db,
    user_id: UserId,
    is_managed: Option<bool>,
    content_rating_ceiling: Option<Option<String>>,
    library_ids: Option<Vec<LibraryId>>,
) -> Result<()> {
    if is_managed.is_some() || content_rating_ceiling.is_some() {
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new("UPDATE users SET ");
        let mut sep = qb.separated(", ");
        if let Some(managed) = is_managed {
            sep.push("is_managed = ");
            sep.push_bind_unseparated(if managed { 1_i64 } else { 0_i64 });
        }
        if let Some(ref ceiling) = content_rating_ceiling {
            sep.push("content_rating_ceiling = ");
            sep.push_bind_unseparated(ceiling.as_deref());
        }
        qb.push(" WHERE id = ");
        qb.push_bind(user_id.0);
        qb.build().execute(&db.write).await?;
    }

    if let Some(libs) = library_ids {
        set_user_libraries(db, user_id, &libs).await?;
    }

    Ok(())
}

pub async fn set_user_pin(db: &Db, user_id: UserId, pin_hash: Option<&str>) -> Result<()> {
    sqlx::query("UPDATE users SET pin_hash = ? WHERE id = ?")
        .bind(pin_hash)
        .bind(user_id.0)
        .execute(&db.write)
        .await?;
    Ok(())
}

pub async fn get_user_pin_hash(db: &Db, user_id: UserId) -> Result<Option<String>> {
    let row = sqlx::query("SELECT pin_hash FROM users WHERE id = ?")
        .bind(user_id.0)
        .fetch_optional(&db.read)
        .await?;
    Ok(row.and_then(|r| r.get("pin_hash")))
}

pub async fn set_user_totp(
    db: &Db,
    user_id: UserId,
    secret: Option<&str>,
    enabled: bool,
) -> Result<()> {
    sqlx::query("UPDATE users SET totp_secret = ?, totp_enabled = ? WHERE id = ?")
        .bind(secret)
        .bind(if enabled { 1_i64 } else { 0_i64 })
        .bind(user_id.0)
        .execute(&db.write)
        .await?;
    Ok(())
}

pub async fn get_user_totp(db: &Db, user_id: UserId) -> Result<Option<(Option<String>, bool)>> {
    let row = sqlx::query("SELECT totp_secret, totp_enabled FROM users WHERE id = ?")
        .bind(user_id.0)
        .fetch_optional(&db.read)
        .await?;
    Ok(row.map(|r| {
        let secret: Option<String> = r.get("totp_secret");
        let enabled: bool = r.get::<i64, _>("totp_enabled") != 0;
        (secret, enabled)
    }))
}

pub async fn get_item_content_rating(db: &Db, item_id: ItemId) -> Result<Option<String>> {
    let row = sqlx::query("SELECT content_rating FROM media_items WHERE id = ?")
        .bind(item_id.0)
        .fetch_optional(&db.read)
        .await?;
    Ok(row.and_then(|r| r.get("content_rating")))
}

// -----------------------------------------------------------------------------
// 3. Invites (M6.2)
// -----------------------------------------------------------------------------

pub async fn create_invite(
    db: &Db,
    code: &str,
    created_by: UserId,
    library_ids: &[LibraryId],
    content_rating_ceiling: Option<&str>,
    expires_at: i64,
) -> Result<InviteRow> {
    let now = now_ms();
    let libs_json = serde_json::to_string(library_ids).map_err(mbl_core::Error::internal)?;

    let id = sqlx::query(
        r#"INSERT INTO invites (code, created_by, library_ids, content_rating_ceiling, expires_at, created_at)
           VALUES (?, ?, ?, ?, ?, ?) RETURNING id"#,
    )
    .bind(code)
    .bind(created_by.0)
    .bind(libs_json)
    .bind(content_rating_ceiling)
    .bind(expires_at)
    .bind(now)
    .fetch_one(&db.write)
    .await?
    .get::<i64, _>("id");

    Ok(InviteRow {
        id,
        code: code.to_string(),
        created_by,
        library_ids: library_ids.to_vec(),
        content_rating_ceiling: content_rating_ceiling.map(|s| s.to_string()),
        expires_at,
        used_at: None,
        used_by: None,
        created_at: now,
    })
}

pub async fn get_invite_by_code(db: &Db, code: &str) -> Result<Option<InviteRow>> {
    let row = sqlx::query(
        r#"SELECT id, code, created_by, library_ids, content_rating_ceiling, expires_at, used_at, used_by, created_at
           FROM invites WHERE code = ?"#,
    )
    .bind(code)
    .fetch_optional(&db.read)
    .await?;

    let Some(r) = row else { return Ok(None) };
    let libs_str: String = r.get("library_ids");
    let library_ids: Vec<LibraryId> = serde_json::from_str(&libs_str).unwrap_or_default();

    Ok(Some(InviteRow {
        id: r.get("id"),
        code: r.get("code"),
        created_by: UserId(r.get("created_by")),
        library_ids,
        content_rating_ceiling: r.get("content_rating_ceiling"),
        expires_at: r.get("expires_at"),
        used_at: r.get("used_at"),
        used_by: r.get::<Option<i64>, _>("used_by").map(UserId),
        created_at: r.get("created_at"),
    }))
}

pub async fn list_invites(db: &Db) -> Result<Vec<InviteRow>> {
    let rows = sqlx::query(
        r#"SELECT id, code, created_by, library_ids, content_rating_ceiling, expires_at, used_at, used_by, created_at
           FROM invites ORDER BY created_at DESC"#,
    )
    .fetch_all(&db.read)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let libs_str: String = r.get("library_ids");
        let library_ids: Vec<LibraryId> = serde_json::from_str(&libs_str).unwrap_or_default();
        out.push(InviteRow {
            id: r.get("id"),
            code: r.get("code"),
            created_by: UserId(r.get("created_by")),
            library_ids,
            content_rating_ceiling: r.get("content_rating_ceiling"),
            expires_at: r.get("expires_at"),
            used_at: r.get("used_at"),
            used_by: r.get::<Option<i64>, _>("used_by").map(UserId),
            created_at: r.get("created_at"),
        });
    }
    Ok(out)
}

pub async fn mark_invite_used(db: &Db, code: &str, used_by: UserId, now: i64) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE invites SET used_at = ?, used_by = ? WHERE code = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(now)
    .bind(used_by.0)
    .bind(code)
    .bind(now)
    .execute(&db.write)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn delete_invite(db: &Db, id: i64) -> Result<bool> {
    let res = sqlx::query("DELETE FROM invites WHERE id = ?").bind(id).execute(&db.write).await?;
    Ok(res.rows_affected() > 0)
}

// -----------------------------------------------------------------------------
// 4. Remote Settings (M6.4 - M6.12)
// -----------------------------------------------------------------------------

pub async fn get_remote_setting(db: &Db, key: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT value FROM remote_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&db.read)
        .await?;
    Ok(row.and_then(|r| r.get("value")))
}

pub async fn set_remote_setting(db: &Db, key: &str, value: &str) -> Result<()> {
    let now = now_ms();
    sqlx::query(
        r#"INSERT INTO remote_settings (key, value, updated_at) VALUES (?, ?, ?)
           ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"#,
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(&db.write)
    .await?;
    Ok(())
}

pub async fn list_remote_settings(db: &Db) -> Result<HashMap<String, String>> {
    let rows = sqlx::query("SELECT key, value FROM remote_settings").fetch_all(&db.read).await?;
    let mut map = HashMap::new();
    for r in rows {
        map.insert(r.get("key"), r.get("value"));
    }
    Ok(map)
}

// -----------------------------------------------------------------------------
// 5. OAuth 2.1 for MCP (M6.17)
// -----------------------------------------------------------------------------

pub async fn register_oauth_client(db: &Db, client: &OAuthClient) -> Result<()> {
    let uris = serde_json::to_string(&client.redirect_uris).map_err(mbl_core::Error::internal)?;
    let grants = serde_json::to_string(&client.grant_types).map_err(mbl_core::Error::internal)?;
    let resps = serde_json::to_string(&client.response_types).map_err(mbl_core::Error::internal)?;

    sqlx::query(
        r#"INSERT INTO oauth_clients (client_id, client_name, redirect_uris, grant_types, response_types, token_endpoint_auth, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&client.client_id)
    .bind(&client.client_name)
    .bind(uris)
    .bind(grants)
    .bind(resps)
    .bind(&client.token_endpoint_auth)
    .bind(client.created_at)
    .execute(&db.write)
    .await?;

    Ok(())
}

pub async fn get_oauth_client(db: &Db, client_id: &str) -> Result<Option<OAuthClient>> {
    let row = sqlx::query(
        r#"SELECT client_id, client_name, redirect_uris, grant_types, response_types, token_endpoint_auth, created_at
           FROM oauth_clients WHERE client_id = ?"#,
    )
    .bind(client_id)
    .fetch_optional(&db.read)
    .await?;

    let Some(r) = row else { return Ok(None) };
    let uris_str: String = r.get("redirect_uris");
    let grants_str: String = r.get("grant_types");
    let resps_str: String = r.get("response_types");

    Ok(Some(OAuthClient {
        client_id: r.get("client_id"),
        client_name: r.get("client_name"),
        redirect_uris: serde_json::from_str(&uris_str).unwrap_or_default(),
        grant_types: serde_json::from_str(&grants_str).unwrap_or_default(),
        response_types: serde_json::from_str(&resps_str).unwrap_or_default(),
        token_endpoint_auth: r.get("token_endpoint_auth"),
        created_at: r.get("created_at"),
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn create_oauth_auth_code(
    db: &Db,
    code: &str,
    code_challenge: &str,
    code_challenge_method: &str,
    client_id: &str,
    user_id: UserId,
    scopes: &[String],
    redirect_uri: &str,
    expires_at: i64,
) -> Result<i64> {
    let now = now_ms();
    let scopes_json = serde_json::to_string(scopes).map_err(mbl_core::Error::internal)?;

    let id = sqlx::query(
        r#"INSERT INTO oauth_grants (code, code_challenge, code_challenge_method, client_id, user_id, scopes, redirect_uri, code_expires_at, created_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"#,
    )
    .bind(code)
    .bind(code_challenge)
    .bind(code_challenge_method)
    .bind(client_id)
    .bind(user_id.0)
    .bind(scopes_json)
    .bind(redirect_uri)
    .bind(expires_at)
    .bind(now)
    .fetch_one(&db.write)
    .await?
    .get::<i64, _>("id");

    Ok(id)
}

pub async fn exchange_oauth_code_for_tokens(
    db: &Db,
    grant_id: i64,
    access_token_hash: &[u8],
    refresh_token_hash: Option<&[u8]>,
    access_expires_at: i64,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE oauth_grants
           SET code = NULL, code_expires_at = NULL, access_token_hash = ?, refresh_token_hash = ?, access_token_expires_at = ?
           WHERE id = ?"#,
    )
    .bind(access_token_hash)
    .bind(refresh_token_hash)
    .bind(access_expires_at)
    .bind(grant_id)
    .execute(&db.write)
    .await?;

    Ok(())
}

pub async fn find_oauth_grant_by_code(db: &Db, code: &str) -> Result<Option<OAuthGrant>> {
    let row = sqlx::query(
        r#"SELECT id, code, code_challenge, code_challenge_method, client_id, user_id, scopes, redirect_uri,
                  access_token_hash, refresh_token_hash, code_expires_at, access_token_expires_at, revoked_at, created_at
           FROM oauth_grants WHERE code = ?"#,
    )
    .bind(code)
    .fetch_optional(&db.read)
    .await?;

    let Some(r) = row else { return Ok(None) };
    let scopes_str: String = r.get("scopes");

    Ok(Some(OAuthGrant {
        id: r.get("id"),
        code: r.get("code"),
        code_challenge: r.get("code_challenge"),
        code_challenge_method: r.get("code_challenge_method"),
        client_id: r.get("client_id"),
        user_id: UserId(r.get("user_id")),
        scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
        redirect_uri: r.get("redirect_uri"),
        access_token_hash: r.get("access_token_hash"),
        refresh_token_hash: r.get("refresh_token_hash"),
        code_expires_at: r.get("code_expires_at"),
        access_token_expires_at: r.get("access_token_expires_at"),
        revoked_at: r.get("revoked_at"),
        created_at: r.get("created_at"),
    }))
}

pub async fn find_oauth_grant_by_access_token_hash(
    db: &Db,
    hash: &[u8],
) -> Result<Option<OAuthGrant>> {
    let row = sqlx::query(
        r#"SELECT id, code, code_challenge, code_challenge_method, client_id, user_id, scopes, redirect_uri,
                  access_token_hash, refresh_token_hash, code_expires_at, access_token_expires_at, revoked_at, created_at
           FROM oauth_grants WHERE access_token_hash = ?"#,
    )
    .bind(hash)
    .fetch_optional(&db.read)
    .await?;

    let Some(r) = row else { return Ok(None) };
    let scopes_str: String = r.get("scopes");

    Ok(Some(OAuthGrant {
        id: r.get("id"),
        code: r.get("code"),
        code_challenge: r.get("code_challenge"),
        code_challenge_method: r.get("code_challenge_method"),
        client_id: r.get("client_id"),
        user_id: UserId(r.get("user_id")),
        scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
        redirect_uri: r.get("redirect_uri"),
        access_token_hash: r.get("access_token_hash"),
        refresh_token_hash: r.get("refresh_token_hash"),
        code_expires_at: r.get("code_expires_at"),
        access_token_expires_at: r.get("access_token_expires_at"),
        revoked_at: r.get("revoked_at"),
        created_at: r.get("created_at"),
    }))
}

pub async fn find_oauth_grant_by_refresh_token_hash(
    db: &Db,
    hash: &[u8],
) -> Result<Option<OAuthGrant>> {
    let row = sqlx::query(
        r#"SELECT id, code, code_challenge, code_challenge_method, client_id, user_id, scopes, redirect_uri,
                  access_token_hash, refresh_token_hash, code_expires_at, access_token_expires_at, revoked_at, created_at
           FROM oauth_grants WHERE refresh_token_hash = ?"#,
    )
    .bind(hash)
    .fetch_optional(&db.read)
    .await?;

    let Some(r) = row else { return Ok(None) };
    let scopes_str: String = r.get("scopes");

    Ok(Some(OAuthGrant {
        id: r.get("id"),
        code: r.get("code"),
        code_challenge: r.get("code_challenge"),
        code_challenge_method: r.get("code_challenge_method"),
        client_id: r.get("client_id"),
        user_id: UserId(r.get("user_id")),
        scopes: serde_json::from_str(&scopes_str).unwrap_or_default(),
        redirect_uri: r.get("redirect_uri"),
        access_token_hash: r.get("access_token_hash"),
        refresh_token_hash: r.get("refresh_token_hash"),
        code_expires_at: r.get("code_expires_at"),
        access_token_expires_at: r.get("access_token_expires_at"),
        revoked_at: r.get("revoked_at"),
        created_at: r.get("created_at"),
    }))
}

pub async fn revoke_oauth_grant_by_token(db: &Db, token_hash: &[u8]) -> Result<bool> {
    let now = now_ms();
    let res = sqlx::query(
        "UPDATE oauth_grants SET revoked_at = ? WHERE (access_token_hash = ? OR refresh_token_hash = ?) AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(token_hash)
    .bind(token_hash)
    .execute(&db.write)
    .await?;
    Ok(res.rows_affected() > 0)
}
