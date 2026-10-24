use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::dto::{SetupRequest, SetupStatus};
use crate::extract::device_info;
use crate::routes::auth::login_response;
use crate::{ApiError, AppState, ServerInfo};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/system/info", get(info))
        .route("/api/v1/system/setup", get(setup_status).post(setup))
}

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
}

/// Liveness + database reachability.
async fn health(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    sqlx::query("SELECT 1")
        .execute(&state.db.read)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "health check: database unavailable");
            mbl_core::Error::Unavailable("database".into())
        })?;
    Ok(Json(Health { status: "ok" }))
}

async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json((*state.info).clone())
}

async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    Ok(Json(SetupStatus {
        needs_setup: state.services.auth.needs_setup().await?,
    }))
}

/// Create the first admin account. Only allowed while no users exist.
async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<Response, ApiError> {
    let device = device_info(&headers, req.device_name, req.client_name);
    let result = state
        .services
        .auth
        .setup(
            &req.username,
            &req.password,
            req.display_name.as_deref(),
            &device,
        )
        .await?;
    Ok(login_response(
        StatusCode::CREATED,
        result,
        state.secure_cookies,
    ))
}
