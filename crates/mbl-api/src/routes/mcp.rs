//! MCP administration routes: audit view, kill-switch toggle, presets, and snippets.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_db::repo::mcp::{AuditFilter, McpAuditEntry};
use mbl_service::{
    McpConfigSnippets, McpPreset, McpSnippetsRequest, McpStatus, UpdateMcpStatusRequest,
};

use crate::extract::{AdminUser, AuthUser};
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/mcp/audit", get(list_audit))
        .route("/api/v1/mcp/status", get(get_status).patch(update_status))
        .route("/api/v1/mcp/snippets", post(create_snippets))
        .route("/api/v1/mcp/presets", get(list_presets))
}

/// Query the Model Context Protocol audit log with optional filters (admin only).
#[utoipa::path(
    get,
    path = "/api/v1/mcp/audit",
    tag = "mcp",
    params(
        ("tokenId" = Option<i64>, Query, description = "Filter by token ID"),
        ("tool" = Option<String>, Query, description = "Filter by tool name"),
        ("outcome" = Option<String>, Query, description = "Filter by outcome ('ok', 'error', 'forbidden', 'rate_limited')"),
        ("limit" = Option<u32>, Query, description = "Max rows to return (default 50, max 500)"),
    ),
    responses(
        (status = 200, body = Vec<McpAuditEntry>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (admin only)"),
    )
)]
async fn list_audit(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Query(filter): Query<AuditFilter>,
) -> Result<Json<Vec<McpAuditEntry>>, ApiError> {
    let entries = state.services.mcp.list_audit(&admin.actor, &filter).await?;
    Ok(Json(entries))
}

/// Inspect the MCP subsystem status and kill switch (admin only).
#[utoipa::path(
    get,
    path = "/api/v1/mcp/status",
    tag = "mcp",
    responses(
        (status = 200, body = McpStatus),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (admin only)"),
    )
)]
async fn get_status(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
) -> Result<Json<McpStatus>, ApiError> {
    let status = state.services.mcp.get_status(&admin.actor)?;
    Ok(Json(status))
}

/// Toggle the global MCP kill switch (admin only).
#[utoipa::path(
    patch,
    path = "/api/v1/mcp/status",
    tag = "mcp",
    request_body = UpdateMcpStatusRequest,
    responses(
        (status = 200, body = McpStatus),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (admin only)"),
    )
)]
async fn update_status(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(req): Json<UpdateMcpStatusRequest>,
) -> Result<Json<McpStatus>, ApiError> {
    let status = state.services.mcp.set_enabled(&admin.actor, req.enabled)?;
    Ok(Json(status))
}

/// Generate client configuration snippets for VS Code, Claude Desktop, Cursor, and stdio.
#[utoipa::path(
    post,
    path = "/api/v1/mcp/snippets",
    tag = "mcp",
    request_body = McpSnippetsRequest,
    responses(
        (status = 200, body = McpConfigSnippets),
        (status = 401, description = "Unauthorized"),
    )
)]
async fn create_snippets(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<McpSnippetsRequest>,
) -> Result<Json<McpConfigSnippets>, ApiError> {
    let snippets =
        state.services.mcp.generate_snippets(&auth.actor, req.server_url.as_deref(), &req.token)?;
    Ok(Json(snippets))
}

/// List built-in scope presets for the AI agent Token Wizard.
#[utoipa::path(
    get,
    path = "/api/v1/mcp/presets",
    tag = "mcp",
    responses(
        (status = 200, body = Vec<McpPreset>),
        (status = 401, description = "Unauthorized"),
    )
)]
async fn list_presets(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<McpPreset>>, ApiError> {
    let presets = state.services.mcp.presets(&auth.actor);
    Ok(Json(presets))
}
