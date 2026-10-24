use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::{ApiError, AppState, ServerInfo};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/system/info", get(info))
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
