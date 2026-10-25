//! Hardware acceleration reporting, live transcode capacity, and self-test trigger endpoints (M4.3, M4.11, M4.15).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_hwaccel::TranscodeCapacity;
use mbl_service::{HwReport, HwSelftestResponse};

use crate::extract::{AdminUser, AuthUser};
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/hw/report", get(get_report))
        .route("/api/v1/hw/capacity", get(get_capacity))
        .route("/api/v1/hw/test", post(run_test))
}

/// Get hardware acceleration report, self-test capabilities, Hailo NPU sharing state, and recent failures.
#[utoipa::path(
    get,
    path = "/api/v1/hw/report",
    tag = "hardware",
    responses((status = 200, body = HwReport), (status = 401))
)]
pub(crate) async fn get_report(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<HwReport>, ApiError> {
    Ok(Json(state.services.hw.get_report(&auth.actor).await?))
}

/// Get live hardware/software transcode capacity and available video transcode slots.
#[utoipa::path(
    get,
    path = "/api/v1/hw/capacity",
    tag = "hardware",
    responses((status = 200, body = TranscodeCapacity), (status = 401))
)]
pub(crate) async fn get_capacity(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<TranscodeCapacity>, ApiError> {
    Ok(Json(state.services.hw.get_capacity(&auth.actor).await?))
}

/// Run functional decode/encode self-tests across candidate hardware backends (admin only).
#[utoipa::path(
    post,
    path = "/api/v1/hw/test",
    tag = "hardware",
    responses((status = 200, body = HwSelftestResponse), (status = 401), (status = 403))
)]
pub(crate) async fn run_test(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
) -> Result<Json<HwSelftestResponse>, ApiError> {
    Ok(Json(state.services.hw.run_selftest(&admin.actor).await?))
}
