//! Repositories for `api_tokens` and `mcp_audit` (`0002_mcp.sql`).

use mbl_core::{Result, UserId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokenInfo {
    pub id: i64,
    pub user_id: UserId,
    pub name: String,
    pub scopes: Vec<String>,
    pub allowed_networks: Vec<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ApiTokenRow {
    id: i64,
    user_id: UserId,
    name: String,
    scopes: String,
    allowed_networks: String,
    created_at: i64,
    expires_at: Option<i64>,
    last_used_at: Option<i64>,
    revoked_at: Option<i64>,
}

impl From<ApiTokenRow> for ApiTokenInfo {
    fn from(r: ApiTokenRow) -> Self {
        let scopes = serde_json::from_str::<Vec<String>>(&r.scopes).unwrap_or_default();
        let allowed_networks =
            serde_json::from_str::<Vec<String>>(&r.allowed_networks).unwrap_or_default();
        Self {
            id: r.id,
            user_id: r.user_id,
            name: r.name,
            scopes,
            allowed_networks,
            created_at: r.created_at,
            expires_at: r.expires_at,
            last_used_at: r.last_used_at,
            revoked_at: r.revoked_at,
        }
    }
}

/// Token row joined with the owning user's `is_admin` status for authentication.
#[derive(Debug, Clone)]
pub struct ApiTokenAuth {
    pub id: i64,
    pub user_id: UserId,
    pub is_admin: bool,
    pub name: String,
    pub scopes: Vec<String>,
    pub allowed_networks: Vec<String>,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ApiTokenAuthRow {
    id: i64,
    user_id: UserId,
    is_admin: bool,
    name: String,
    scopes: String,
    allowed_networks: String,
    expires_at: Option<i64>,
    last_used_at: Option<i64>,
    revoked_at: Option<i64>,
}

impl From<ApiTokenAuthRow> for ApiTokenAuth {
    fn from(r: ApiTokenAuthRow) -> Self {
        Self {
            id: r.id,
            user_id: r.user_id,
            is_admin: r.is_admin,
            name: r.name,
            scopes: serde_json::from_str(&r.scopes).unwrap_or_default(),
            allowed_networks: serde_json::from_str(&r.allowed_networks).unwrap_or_default(),
            expires_at: r.expires_at,
            last_used_at: r.last_used_at,
            revoked_at: r.revoked_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewApiToken<'a> {
    pub user_id: UserId,
    pub name: &'a str,
    pub token_hash: &'a [u8],
    pub scopes_json: &'a str,
    pub allowed_networks_json: &'a str,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct McpAuditEntry {
    pub id: i64,
    pub ts: i64,
    pub user_id: Option<UserId>,
    pub token_id: Option<i64>,
    pub client: Option<String>,
    pub tool: String,
    pub args: String,
    pub outcome: String,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct NewMcpAudit<'a> {
    pub ts: i64,
    pub user_id: Option<UserId>,
    pub token_id: Option<i64>,
    pub client: Option<&'a str>,
    pub tool: &'a str,
    pub args: &'a str,
    pub outcome: &'a str,
    pub error: Option<&'a str>,
    pub duration_ms: i64,
    pub dry_run: bool,
}

pub async fn insert_token(db: &Db, t: &NewApiToken<'_>) -> Result<ApiTokenInfo> {
    let row = sqlx::query_as::<_, ApiTokenRow>(
        "INSERT INTO api_tokens
           (user_id, name, token_hash, scopes, allowed_networks, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id, user_id, name, scopes, allowed_networks, created_at, expires_at, last_used_at, revoked_at",
    )
    .bind(t.user_id)
    .bind(t.name)
    .bind(t.token_hash)
    .bind(t.scopes_json)
    .bind(t.allowed_networks_json)
    .bind(t.created_at)
    .bind(t.expires_at)
    .fetch_one(&db.write)
    .await?;
    Ok(row.into())
}

pub async fn find_by_token_hash(db: &Db, token_hash: &[u8]) -> Result<Option<ApiTokenAuth>> {
    let row = sqlx::query_as::<_, ApiTokenAuthRow>(
        "SELECT t.id, t.user_id, u.is_admin, t.name, t.scopes, t.allowed_networks,
                t.expires_at, t.last_used_at, t.revoked_at
         FROM api_tokens t
         JOIN users u ON u.id = t.user_id
         WHERE t.token_hash = ?",
    )
    .bind(token_hash)
    .fetch_optional(&db.read)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn touch_token(db: &Db, id: i64, now: i64) -> Result<()> {
    sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&db.write)
        .await?;
    Ok(())
}

pub async fn list_tokens_for_user(db: &Db, user_id: UserId) -> Result<Vec<ApiTokenInfo>> {
    let rows = sqlx::query_as::<_, ApiTokenRow>(
        "SELECT id, user_id, name, scopes, allowed_networks, created_at, expires_at, last_used_at, revoked_at
         FROM api_tokens
         WHERE user_id = ? AND revoked_at IS NULL
         ORDER BY created_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(&db.read)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn list_all_tokens(db: &Db) -> Result<Vec<ApiTokenInfo>> {
    let rows = sqlx::query_as::<_, ApiTokenRow>(
        "SELECT id, user_id, name, scopes, allowed_networks, created_at, expires_at, last_used_at, revoked_at
         FROM api_tokens
         ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(&db.read)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn revoke_token(
    db: &Db,
    user_id: UserId,
    is_admin: bool,
    id: i64,
    now: i64,
) -> Result<bool> {
    let res = if is_admin {
        sqlx::query("UPDATE api_tokens SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
            .bind(now)
            .bind(id)
            .execute(&db.write)
            .await?
    } else {
        sqlx::query(
            "UPDATE api_tokens SET revoked_at = ? WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&db.write)
        .await?
    };
    Ok(res.rows_affected() > 0)
}

pub async fn insert_audit(db: &Db, a: &NewMcpAudit<'_>) -> Result<i64> {
    let dry_run_int: i64 = i64::from(a.dry_run);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO mcp_audit
           (ts, user_id, token_id, client, tool, args, outcome, error, duration_ms, dry_run)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(a.ts)
    .bind(a.user_id)
    .bind(a.token_id)
    .bind(a.client)
    .bind(a.tool)
    .bind(a.args)
    .bind(a.outcome)
    .bind(a.error)
    .bind(a.duration_ms)
    .bind(dry_run_int)
    .fetch_one(&db.write)
    .await?;
    Ok(id)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AuditFilter {
    pub token_id: Option<i64>,
    pub tool: Option<String>,
    pub outcome: Option<String>,
    pub limit: Option<u32>,
}

pub async fn list_audit(db: &Db, limit: u32) -> Result<Vec<McpAuditEntry>> {
    list_audit_filtered(db, &AuditFilter { limit: Some(limit), ..Default::default() }).await
}

pub async fn list_audit_filtered(db: &Db, filter: &AuditFilter) -> Result<Vec<McpAuditEntry>> {
    let lim = i64::from(filter.limit.unwrap_or(50).clamp(1, 500));
    let rows = sqlx::query_as::<_, McpAuditEntry>(
        "SELECT id, ts, user_id, token_id, client, tool, args, outcome, error, duration_ms,
                (dry_run != 0) AS dry_run
         FROM mcp_audit
         WHERE (?1 IS NULL OR token_id = ?1)
           AND (?2 IS NULL OR tool = ?2)
           AND (?3 IS NULL OR outcome = ?3)
         ORDER BY ts DESC, id DESC
         LIMIT ?4",
    )
    .bind(filter.token_id)
    .bind(filter.tool.as_deref())
    .bind(filter.outcome.as_deref())
    .bind(lim)
    .fetch_all(&db.read)
    .await?;
    Ok(rows)
}

pub async fn prune_audit_before(db: &Db, cutoff_ms: i64) -> Result<u64> {
    let res = sqlx::query("DELETE FROM mcp_audit WHERE ts < ?")
        .bind(cutoff_ms)
        .execute(&db.write)
        .await?;
    Ok(res.rows_affected())
}
