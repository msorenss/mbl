//! Milestone M8 database repositories:
//! - Transcode Workers (`workers`, `worker_pairing_codes`)
//! - Remote Library Nodes (`remote_nodes`, `remote_node_libraries`, `remote_node_pairing_codes`)

use mbl_core::{
    NodeCapabilities, NodeId, NodeLibrary, Result, UserId, WorkerCapabilities, WorkerId,
    WorkerPathMapping, time::now_ms,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

use crate::Db;

// -----------------------------------------------------------------------------
// 1. Transcode Workers (M8.1, M8.2, M8.5, M8.7)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRow {
    pub id: WorkerId,
    pub name: String,
    pub hostname: Option<String>,
    pub mac_address: Option<String>,
    pub cert_fingerprint: Option<String>,
    pub capabilities: WorkerCapabilities,
    pub max_sessions: u32,
    pub status: String,
    pub last_heartbeat_at: Option<i64>,
    pub path_mappings: Vec<WorkerPathMapping>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub async fn create_worker_pairing_code(
    db: &Db,
    code: &str,
    created_by: UserId,
    expires_at: i64,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO worker_pairing_codes (code, created_by, expires_at)
           VALUES (?, ?, ?)"#,
    )
    .bind(code)
    .bind(created_by.0)
    .bind(expires_at)
    .execute(&db.write)
    .await?;
    Ok(())
}

pub async fn consume_worker_pairing_code(db: &Db, code: &str) -> Result<Option<UserId>> {
    let now = now_ms();
    let row = sqlx::query(
        r#"SELECT created_by, expires_at
           FROM worker_pairing_codes
           WHERE code = ?"#,
    )
    .bind(code)
    .fetch_optional(&db.read)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let expires_at: i64 = row.try_get("expires_at")?;
    let created_by: i64 = row.try_get("created_by")?;

    // Delete code once checked
    sqlx::query("DELETE FROM worker_pairing_codes WHERE code = ?")
        .bind(code)
        .execute(&db.write)
        .await?;

    if now > expires_at {
        return Ok(None);
    }

    Ok(Some(UserId(created_by)))
}

#[allow(clippy::too_many_arguments)]
pub async fn register_worker(
    db: &Db,
    name: &str,
    hostname: Option<&str>,
    mac_address: Option<&str>,
    token_hash: &[u8],
    cert_fingerprint: Option<&str>,
    caps: &WorkerCapabilities,
    max_sessions: u32,
    path_mappings: &[WorkerPathMapping],
) -> Result<WorkerId> {
    let now = now_ms();
    let caps_json = serde_json::to_string(caps).unwrap_or_else(|_| "{}".into());
    let mappings_json = serde_json::to_string(path_mappings).unwrap_or_else(|_| "[]".into());

    let res = sqlx::query(
        r#"INSERT INTO workers (
               name, hostname, mac_address, auth_token_hash, cert_fingerprint,
               capabilities, max_sessions, status, last_heartbeat_at, path_mappings,
               created_at, updated_at
           )
           VALUES (?, ?, ?, ?, ?, ?, ?, 'idle', ?, ?, ?, ?)"#,
    )
    .bind(name)
    .bind(hostname)
    .bind(mac_address)
    .bind(token_hash)
    .bind(cert_fingerprint)
    .bind(caps_json)
    .bind(max_sessions as i64)
    .bind(now)
    .bind(mappings_json)
    .bind(now)
    .bind(now)
    .execute(&db.write)
    .await?;

    Ok(WorkerId(res.last_insert_rowid()))
}

fn parse_worker_row(row: sqlx::sqlite::SqliteRow) -> Result<WorkerRow> {
    let caps_raw: String = row.try_get("capabilities")?;
    let mappings_raw: String = row.try_get("path_mappings")?;

    let capabilities: WorkerCapabilities = serde_json::from_str(&caps_raw).unwrap_or_default();
    let path_mappings: Vec<WorkerPathMapping> =
        serde_json::from_str(&mappings_raw).unwrap_or_default();

    Ok(WorkerRow {
        id: WorkerId(row.try_get("id")?),
        name: row.try_get("name")?,
        hostname: row.try_get("hostname")?,
        mac_address: row.try_get("mac_address")?,
        cert_fingerprint: row.try_get("cert_fingerprint")?,
        capabilities,
        max_sessions: row.try_get::<i64, _>("max_sessions")? as u32,
        status: row.try_get("status")?,
        last_heartbeat_at: row.try_get("last_heartbeat_at")?,
        path_mappings,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn get_worker(db: &Db, id: WorkerId) -> Result<Option<WorkerRow>> {
    let row = sqlx::query("SELECT * FROM workers WHERE id = ?")
        .bind(id.0)
        .fetch_optional(&db.read)
        .await?;

    row.map(parse_worker_row).transpose()
}

pub async fn get_worker_by_token_hash(db: &Db, token_hash: &[u8]) -> Result<Option<WorkerRow>> {
    let row = sqlx::query("SELECT * FROM workers WHERE auth_token_hash = ?")
        .bind(token_hash)
        .fetch_optional(&db.read)
        .await?;

    row.map(parse_worker_row).transpose()
}

pub async fn list_workers(db: &Db) -> Result<Vec<WorkerRow>> {
    let rows = sqlx::query("SELECT * FROM workers ORDER BY id ASC").fetch_all(&db.read).await?;

    rows.into_iter().map(parse_worker_row).collect()
}

pub async fn update_worker_heartbeat(
    db: &Db,
    id: WorkerId,
    status: &str,
    last_heartbeat_at: i64,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE workers
           SET status = ?, last_heartbeat_at = ?, updated_at = ?
           WHERE id = ?"#,
    )
    .bind(status)
    .bind(last_heartbeat_at)
    .bind(now_ms())
    .bind(id.0)
    .execute(&db.write)
    .await?;

    Ok(())
}

pub async fn update_worker_path_mappings(
    db: &Db,
    id: WorkerId,
    mappings: &[WorkerPathMapping],
) -> Result<()> {
    let mappings_json = serde_json::to_string(mappings).unwrap_or_else(|_| "[]".into());
    sqlx::query("UPDATE workers SET path_mappings = ?, updated_at = ? WHERE id = ?")
        .bind(mappings_json)
        .bind(now_ms())
        .bind(id.0)
        .execute(&db.write)
        .await?;

    Ok(())
}

pub async fn delete_worker(db: &Db, id: WorkerId) -> Result<bool> {
    let res = sqlx::query("DELETE FROM workers WHERE id = ?").bind(id.0).execute(&db.write).await?;

    Ok(res.rows_affected() > 0)
}

// -----------------------------------------------------------------------------
// 2. Remote Library Nodes / Federated Storage Nodes (M8.10, M8.11)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoteNodeRow {
    pub id: NodeId,
    pub name: String,
    pub base_url: String,
    pub status: String,
    pub last_seen_at: Option<i64>,
    pub capabilities: NodeCapabilities,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoteNodeLibraryRow {
    pub id: i64,
    pub node_id: NodeId,
    pub remote_library_id: i64,
    pub name: String,
    pub kind: String,
    pub item_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

pub async fn create_node_pairing_code(
    db: &Db,
    code: &str,
    created_by: UserId,
    expires_at: i64,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO remote_node_pairing_codes (code, created_by, expires_at)
           VALUES (?, ?, ?)"#,
    )
    .bind(code)
    .bind(created_by.0)
    .bind(expires_at)
    .execute(&db.write)
    .await?;
    Ok(())
}

pub async fn consume_node_pairing_code(db: &Db, code: &str) -> Result<Option<UserId>> {
    let now = now_ms();
    let row = sqlx::query(
        r#"SELECT created_by, expires_at
           FROM remote_node_pairing_codes
           WHERE code = ?"#,
    )
    .bind(code)
    .fetch_optional(&db.read)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let expires_at: i64 = row.try_get("expires_at")?;
    let created_by: i64 = row.try_get("created_by")?;

    sqlx::query("DELETE FROM remote_node_pairing_codes WHERE code = ?")
        .bind(code)
        .execute(&db.write)
        .await?;

    if now > expires_at {
        return Ok(None);
    }

    Ok(Some(UserId(created_by)))
}

pub async fn register_node(
    db: &Db,
    name: &str,
    base_url: &str,
    token_hash: &[u8],
    caps: &NodeCapabilities,
) -> Result<NodeId> {
    let now = now_ms();
    let caps_json = serde_json::to_string(caps).unwrap_or_else(|_| "{}".into());

    let res = sqlx::query(
        r#"INSERT INTO remote_nodes (
               name, base_url, auth_token_hash, status, last_seen_at, capabilities, created_at
           )
           VALUES (?, ?, ?, 'online', ?, ?, ?)"#,
    )
    .bind(name)
    .bind(base_url)
    .bind(token_hash)
    .bind(now)
    .bind(caps_json)
    .bind(now)
    .execute(&db.write)
    .await?;

    Ok(NodeId(res.last_insert_rowid()))
}

fn parse_node_row(row: sqlx::sqlite::SqliteRow) -> Result<RemoteNodeRow> {
    let caps_raw: String = row.try_get("capabilities")?;
    let capabilities: NodeCapabilities = serde_json::from_str(&caps_raw).unwrap_or_default();

    Ok(RemoteNodeRow {
        id: NodeId(row.try_get("id")?),
        name: row.try_get("name")?,
        base_url: row.try_get("base_url")?,
        status: row.try_get("status")?,
        last_seen_at: row.try_get("last_seen_at")?,
        capabilities,
        created_at: row.try_get("created_at")?,
    })
}

pub async fn get_node(db: &Db, id: NodeId) -> Result<Option<RemoteNodeRow>> {
    let row = sqlx::query("SELECT * FROM remote_nodes WHERE id = ?")
        .bind(id.0)
        .fetch_optional(&db.read)
        .await?;

    row.map(parse_node_row).transpose()
}

pub async fn get_node_by_token_hash(db: &Db, token_hash: &[u8]) -> Result<Option<RemoteNodeRow>> {
    let row = sqlx::query("SELECT * FROM remote_nodes WHERE auth_token_hash = ?")
        .bind(token_hash)
        .fetch_optional(&db.read)
        .await?;

    row.map(parse_node_row).transpose()
}

pub async fn list_nodes(db: &Db) -> Result<Vec<RemoteNodeRow>> {
    let rows =
        sqlx::query("SELECT * FROM remote_nodes ORDER BY id ASC").fetch_all(&db.read).await?;

    rows.into_iter().map(parse_node_row).collect()
}

pub async fn update_node_heartbeat(
    db: &Db,
    id: NodeId,
    status: &str,
    last_seen_at: i64,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE remote_nodes
           SET status = ?, last_seen_at = ?
           WHERE id = ?"#,
    )
    .bind(status)
    .bind(last_seen_at)
    .bind(id.0)
    .execute(&db.write)
    .await?;

    Ok(())
}

pub async fn delete_node(db: &Db, id: NodeId) -> Result<bool> {
    let res =
        sqlx::query("DELETE FROM remote_nodes WHERE id = ?").bind(id.0).execute(&db.write).await?;

    Ok(res.rows_affected() > 0)
}

pub async fn sync_node_libraries(
    db: &Db,
    node_id: NodeId,
    libraries: &[NodeLibrary],
) -> Result<(u32, i64)> {
    let now = now_ms();
    let mut total_items = 0i64;

    // Delete existing remote libraries for this node and replace with the new snapshot
    sqlx::query("DELETE FROM remote_node_libraries WHERE node_id = ?")
        .bind(node_id.0)
        .execute(&db.write)
        .await?;

    for lib in libraries {
        total_items += lib.item_count;
        sqlx::query(
            r#"INSERT INTO remote_node_libraries (
                   node_id, remote_library_id, name, kind, item_count, created_at, updated_at
               )
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(node_id.0)
        .bind(lib.remote_library_id)
        .bind(&lib.name)
        .bind(&lib.kind)
        .bind(lib.item_count)
        .bind(now)
        .bind(now)
        .execute(&db.write)
        .await?;
    }

    Ok((libraries.len() as u32, total_items))
}

fn parse_node_lib_row(row: sqlx::sqlite::SqliteRow) -> Result<RemoteNodeLibraryRow> {
    Ok(RemoteNodeLibraryRow {
        id: row.try_get("id")?,
        node_id: NodeId(row.try_get("node_id")?),
        remote_library_id: row.try_get("remote_library_id")?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        item_count: row.try_get("item_count")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn list_node_libraries(db: &Db, node_id: NodeId) -> Result<Vec<RemoteNodeLibraryRow>> {
    let rows = sqlx::query(
        "SELECT * FROM remote_node_libraries WHERE node_id = ? ORDER BY remote_library_id ASC",
    )
    .bind(node_id.0)
    .fetch_all(&db.read)
    .await?;

    rows.into_iter().map(parse_node_lib_row).collect()
}

pub async fn list_all_node_libraries(db: &Db) -> Result<Vec<RemoteNodeLibraryRow>> {
    let rows = sqlx::query("SELECT * FROM remote_node_libraries ORDER BY node_id, id ASC")
        .fetch_all(&db.read)
        .await?;

    rows.into_iter().map(parse_node_lib_row).collect()
}
