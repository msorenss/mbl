//! Scheduled database upkeep and cleanup (admin).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_service::maintenance::{
    MaintenanceReport, MaintenanceSettings, RunMaintenance, UpdateMaintenanceSettings,
};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/system/maintenance", get(get_maintenance).put(update_maintenance))
        .route("/api/v1/system/maintenance/run", post(run_maintenance))
}

#[utoipa::path(get, path = "/api/v1/system/maintenance", operation_id = "get_maintenance", tag = "system",
    responses((status = 200, body = MaintenanceSettings), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn get_maintenance(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<MaintenanceSettings>, ApiError> {
    Ok(Json(state.services.maintenance.get_settings(&auth.actor).await?))
}

#[utoipa::path(put, path = "/api/v1/system/maintenance", operation_id = "update_maintenance", tag = "system",
    request_body = UpdateMaintenanceSettings,
    responses((status = 200, body = MaintenanceSettings), (status = 403), (status = 422)),
    security(("bearer_auth" = [])))]
pub(crate) async fn update_maintenance(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateMaintenanceSettings>,
) -> Result<Json<MaintenanceSettings>, ApiError> {
    Ok(Json(state.services.maintenance.update_settings(&auth.actor, body).await?))
}

/// Run maintenance now, or preview it with `dryRun`. With `cleanup`, titles whose files have been
/// gone for the chosen number of days are removed (never in libraries whose folders cannot be
/// read).
#[utoipa::path(post, path = "/api/v1/system/maintenance/run", operation_id = "run_maintenance", tag = "system",
    request_body = RunMaintenance,
    responses((status = 200, body = MaintenanceReport), (status = 403),
        (status = 409, description = "Maintenance is already running")),
    security(("bearer_auth" = [])))]
pub(crate) async fn run_maintenance(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<RunMaintenance>,
) -> Result<Json<MaintenanceReport>, ApiError> {
    Ok(Json(state.services.maintenance.run(&auth.actor, body).await?))
}
