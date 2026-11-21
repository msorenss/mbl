//! Install AI models for a Hailo NPU from the Hardware page (admin).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mbl_service::hailo_models::HailoModelsStatus;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/hw/models", get(hailo_models))
        .route("/api/v1/hw/models/{file}/download", post(download_hailo_model))
        .route("/api/v1/hw/models/{file}", delete(remove_hailo_model))
        .route("/api/v1/hw/npu", get(npu_activity).put(set_npu))
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetNpu {
    pub enabled: bool,
}

/// Whether MBL may use the NPU, what it has loaded and how often each model ran since start.
#[utoipa::path(get, path = "/api/v1/hw/npu", operation_id = "npu_activity", tag = "hardware",
    responses((status = 200, body = mbl_hailo::NpuActivity), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn npu_activity(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<mbl_hailo::NpuActivity>, ApiError> {
    Ok(Json(state.services.hailo_models.npu(&auth.actor)?))
}

/// Turn MBL's use of the NPU on or off. Off releases everything MBL holds on the board at once.
#[utoipa::path(put, path = "/api/v1/hw/npu", operation_id = "set_npu", tag = "hardware",
    request_body = SetNpu,
    responses((status = 200, body = mbl_hailo::NpuActivity), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn set_npu(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<SetNpu>,
) -> Result<Json<mbl_hailo::NpuActivity>, ApiError> {
    Ok(Json(state.services.hailo_models.set_npu(&auth.actor, body.enabled).await?))
}

/// NPU, HailoRT version and the models MBL can use: size, installed, in use, and the running
/// download.
#[utoipa::path(get, path = "/api/v1/hw/models", operation_id = "hailo_models", tag = "hardware",
    responses((status = 200, body = HailoModelsStatus), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn hailo_models(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<HailoModelsStatus>, ApiError> {
    Ok(Json(state.services.hailo_models.status(&auth.actor).await?))
}

/// Download a model from Hailo's server for the installed HailoRT version, in the background.
#[utoipa::path(post, path = "/api/v1/hw/models/{file}/download", operation_id = "download_hailo_model", tag = "hardware",
    params(("file" = String, Path, description = "Model file, e.g. Qwen2-VL-2B-Instruct.hef")),
    responses((status = 202, description = "Downloading; follow it in GET /hw/models"), (status = 403), (status = 404),
        (status = 409, description = "Another model is downloading"),
        (status = 422, description = "No Hailo-10H, no HailoRT, or not enough space")),
    security(("bearer_auth" = [])))]
pub(crate) async fn download_hailo_model(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(file): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.hailo_models.download(&auth.actor, &file).await?;
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(delete, path = "/api/v1/hw/models/{file}", operation_id = "remove_hailo_model", tag = "hardware",
    params(("file" = String, Path)),
    responses((status = 204), (status = 403), (status = 404), (status = 409)),
    security(("bearer_auth" = [])))]
pub(crate) async fn remove_hailo_model(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(file): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.hailo_models.remove(&auth.actor, &file).await?;
    Ok(StatusCode::NO_CONTENT)
}
