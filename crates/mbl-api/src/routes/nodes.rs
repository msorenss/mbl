//! Remote Library Node API endpoints (M8.10, M8.11): pairing, status, catalog delta sync,
//! and LAN direct play handover.

use axum::extract::{ConnectInfo, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::{
    Error, ItemId, NodeCatalogSyncRequest, NodeCatalogSyncResponse, NodeHandoverResponse, NodeId,
    NodePairRequest, NodePairResponse,
};
use mbl_db::repo::m8::{RemoteNodeLibraryRow, RemoteNodeRow};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodePairingCodeResponse {
    pub code: String,
    pub expires_in_secs: u32,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/nodes/pairing-code", post(generate_node_pairing_code))
        .route("/api/v1/nodes/pair", post(pair_node))
        .route("/api/v1/nodes", get(list_nodes))
        .route("/api/v1/nodes/{id}", get(get_node).delete(delete_node))
        .route("/api/v1/nodes/{id}/sync-catalog", post(sync_catalog))
        .route("/api/v1/nodes/{id}/libraries", get(list_libraries))
        .route("/api/v1/nodes/{id}/stream/handover/{itemId}", get(smart_handover))
}

#[utoipa::path(
    post,
    path = "/api/v1/nodes/pairing-code",
    responses(
        (status = 200, description = "Pairing code generated", body = NodePairingCodeResponse),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn generate_node_pairing_code(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<NodePairingCodeResponse>, ApiError> {
    let code = state.services.nodes.generate_pairing_code(&auth.actor).await?;
    Ok(Json(NodePairingCodeResponse { code, expires_in_secs: 900 }))
}

#[utoipa::path(
    post,
    path = "/api/v1/nodes/pair",
    request_body = NodePairRequest,
    responses(
        (status = 200, description = "Remote library node paired", body = NodePairResponse),
        (status = 401, description = "Invalid or expired pairing code"),
    )
)]
async fn pair_node(
    State(state): State<AppState>,
    Json(req): Json<NodePairRequest>,
) -> Result<Json<NodePairResponse>, ApiError> {
    let resp = state.services.nodes.pair_node(req).await?;
    Ok(Json(resp))
}

#[utoipa::path(
    get,
    path = "/api/v1/nodes",
    responses(
        (status = 200, description = "List registered remote storage nodes", body = Vec<RemoteNodeRow>),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_nodes(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<RemoteNodeRow>>, ApiError> {
    let nodes = state.services.nodes.list_nodes(&auth.actor).await?;
    Ok(Json(nodes))
}

#[utoipa::path(
    get,
    path = "/api/v1/nodes/{id}",
    params(("id" = i64, Path)),
    responses(
        (status = 200, description = "Remote storage node details", body = RemoteNodeRow),
        (status = 404, description = "Node not found"),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_node(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<NodeId>,
) -> Result<Json<RemoteNodeRow>, ApiError> {
    let node = state.services.nodes.get_node(&auth.actor, id).await?;
    Ok(Json(node))
}

#[utoipa::path(
    delete,
    path = "/api/v1/nodes/{id}",
    params(("id" = i64, Path)),
    responses(
        (status = 204, description = "Remote storage node revoked"),
        (status = 404, description = "Node not found"),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_node(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<NodeId>,
) -> Result<StatusCode, ApiError> {
    state.services.nodes.delete_node(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/nodes/{id}/sync-catalog",
    params(("id" = i64, Path)),
    request_body = NodeCatalogSyncRequest,
    responses(
        (status = 200, description = "Catalog synced successfully", body = NodeCatalogSyncResponse),
        (status = 401, description = "Unauthorized node"),
        (status = 404, description = "Node not found"),
    ),
    security(("node_bearer_auth" = []))
)]
async fn sync_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<NodeId>,
    Json(req): Json<NodeCatalogSyncRequest>,
) -> Result<Json<NodeCatalogSyncResponse>, ApiError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;

    let node = state.services.nodes.authenticate(auth_header).await?;
    if node.id != id {
        return Err(Error::Unauthorized.into());
    }

    let resp = state.services.nodes.sync_catalog(id, req).await?;
    Ok(Json(resp))
}

#[utoipa::path(
    get,
    path = "/api/v1/nodes/{id}/libraries",
    params(("id" = i64, Path)),
    responses(
        (status = 200, description = "List synced libraries for node", body = Vec<RemoteNodeLibraryRow>),
        (status = 404, description = "Node not found"),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_libraries(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<NodeId>,
) -> Result<Json<Vec<RemoteNodeLibraryRow>>, ApiError> {
    let libs = state.services.nodes.list_node_libraries(&auth.actor, id).await?;
    Ok(Json(libs))
}

#[utoipa::path(
    get,
    path = "/api/v1/nodes/{id}/stream/handover/{itemId}",
    params(
        ("id" = i64, Path),
        ("itemId" = i64, Path)
    ),
    responses(
        (status = 200, description = "Smart handover evaluation", body = NodeHandoverResponse),
        (status = 404, description = "Node not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn smart_handover(
    auth: _AuthOrIp,
    State(state): State<AppState>,
    AxumPath((id, item_id)): AxumPath<(NodeId, ItemId)>,
) -> Result<Json<NodeHandoverResponse>, ApiError> {
    let handover = state.services.nodes.get_handover(id, item_id, auth.client_ip).await?;
    Ok(Json(handover))
}

/// Helper extractor to obtain client IP address for handover decision
pub struct _AuthOrIp {
    pub client_ip: Option<std::net::IpAddr>,
}

impl<S> axum::extract::FromRequestParts<S> for _AuthOrIp
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let client_ip =
            parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|ci| ci.0.ip()).or_else(|| {
                parts
                    .headers
                    .get("x-forwarded-for")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse().ok())
            });

        Ok(_AuthOrIp { client_ip })
    }
}
