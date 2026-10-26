//! Remote access and bandwidth management endpoints (M6.4 - M6.12).

use axum::extract::State;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use mbl_remote::ReachabilityReport;
use mbl_service::{BandwidthUsageResponse, RemoteStatusResponse, UpdateRemotePolicyRequest};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::extract::AdminUser;
use crate::{ApiError, AppState};

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReachabilityTestRequest {
    pub public_ip_override: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/system/remote-status", get(get_status))
        .route("/api/v1/system/reachability-test", post(test_reachability))
        .route("/api/v1/system/remote-policy", put(update_policy))
        .route("/api/v1/system/bandwidth", get(get_bandwidth))
}

/// Fetch remote access, Tailscale, and WAN policy status (admin only).
#[utoipa::path(get, path = "/api/v1/system/remote-status", tag = "remote", operation_id = "get_remote_status",
    responses((status = 200, body = RemoteStatusResponse), (status = 403)))]
pub(crate) async fn get_status(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
) -> Result<Json<RemoteStatusResponse>, ApiError> {
    Ok(Json(state.services.remote.get_status(&auth.actor).await?))
}

/// Run external reachability and CGNAT analysis (admin only).
#[utoipa::path(post, path = "/api/v1/system/reachability-test", tag = "remote", operation_id = "test_reachability",
    request_body = ReachabilityTestRequest,
    responses((status = 200, body = ReachabilityReport), (status = 403)))]
pub(crate) async fn test_reachability(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Json(req): Json<ReachabilityTestRequest>,
) -> Result<Json<ReachabilityReport>, ApiError> {
    Ok(Json(state.services.remote.test_reachability(&auth.actor, req.public_ip_override).await?))
}

/// Update remote policies (admin LAN-only, Tailscale auth, WAN upload cap) (admin only).
#[utoipa::path(put, path = "/api/v1/system/remote-policy", tag = "remote", operation_id = "update_remote_policy",
    request_body = UpdateRemotePolicyRequest,
    responses((status = 200, body = RemoteStatusResponse), (status = 403)))]
pub(crate) async fn update_policy(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Json(req): Json<UpdateRemotePolicyRequest>,
) -> Result<Json<RemoteStatusResponse>, ApiError> {
    Ok(Json(state.services.remote.update_policy(&auth.actor, req).await?))
}

/// Inspect remote streaming bandwidth fair-share allocation (admin only).
#[utoipa::path(get, path = "/api/v1/system/bandwidth", tag = "remote", operation_id = "get_bandwidth_usage",
    responses((status = 200, body = BandwidthUsageResponse), (status = 403)))]
pub(crate) async fn get_bandwidth(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
) -> Result<Json<BandwidthUsageResponse>, ApiError> {
    Ok(Json(state.services.remote.get_bandwidth_usage(&auth.actor).await?))
}
