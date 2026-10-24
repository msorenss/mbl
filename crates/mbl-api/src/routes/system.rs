use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::dto::{LoginResponse, SetupRequest, SetupStatus};
use crate::extract::device_info;
use crate::routes::auth::login_response;
use crate::{ApiError, AppState, ServerInfo};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/system/info", get(info))
        .route("/api/v1/system/setup", get(setup_status).post(setup))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct Health {
    status: &'static str,
}

/// Liveness + database reachability.
#[utoipa::path(get, path = "/health", tag = "system", security(),
    responses((status = 200, body = Health), (status = 503)))]
async fn health(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    sqlx::query("SELECT 1").execute(&state.db.read).await.map_err(|e| {
        tracing::warn!(error = %e, "health check: database unavailable");
        mbl_core::Error::Unavailable("database".into())
    })?;
    Ok(Json(Health { status: "ok" }))
}

/// Server name, version and platform.
#[utoipa::path(get, path = "/api/v1/system/info", tag = "system", security(),
    responses((status = 200, body = ServerInfo)))]
async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json((*state.info).clone())
}

/// Whether the first admin still has to be created.
#[utoipa::path(get, path = "/api/v1/system/setup", tag = "system", security(),
    responses((status = 200, body = SetupStatus)))]
async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    Ok(Json(SetupStatus { needs_setup: state.services.auth.needs_setup().await? }))
}

/// Create the first admin account. Only allowed while no users exist.
#[utoipa::path(post, path = "/api/v1/system/setup", tag = "system", security(),
    request_body = SetupRequest,
    responses((status = 201, body = LoginResponse), (status = 409)))]
async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<Response, ApiError> {
    let device = device_info(&headers, req.device_name, req.client_name);
    let result = state
        .services
        .auth
        .setup(&req.username, &req.password, req.display_name.as_deref(), &device)
        .await?;
    Ok(login_response(StatusCode::CREATED, result, state.secure_cookies))
}
