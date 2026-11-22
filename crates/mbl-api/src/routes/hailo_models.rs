//! Install AI models for a Hailo NPU from the Hardware page (admin).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mbl_service::hailo_models::HailoModelsStatus;
use mbl_service::hailo_ollama::HailoOllamaStatus;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/hw/models", get(hailo_models))
        .route("/api/v1/hw/models/{file}/download", post(download_hailo_model))
        .route("/api/v1/hw/models/{file}", delete(remove_hailo_model))
        .route("/api/v1/hw/npu", get(npu_activity).put(set_npu))
        .route("/api/v1/hw/ollama", get(hailo_ollama_status))
        .route("/api/v1/hw/ollama/start", post(start_hailo_ollama))
        .route("/api/v1/hw/ollama/stop", post(stop_hailo_ollama))
        .route("/api/v1/hw/ollama/models/{name}/pull", post(pull_hailo_ollama_model))
        .route("/api/v1/hw/ollama/models/{name}", delete(delete_hailo_ollama_model))
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
    let activity = state.services.hailo_models.set_npu(&auth.actor, body.enabled).await?;
    if !body.enabled {
        // Hailo-Ollama holds the board too.
        state.services.hailo_ollama.npu_turned_off().await;
    }
    Ok(Json(activity))
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

/// Hailo-Ollama: whether it is installed and running, its models and the running download.
#[utoipa::path(get, path = "/api/v1/hw/ollama", operation_id = "hailo_ollama_status", tag = "hardware",
    responses((status = 200, body = HailoOllamaStatus), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn hailo_ollama_status(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<HailoOllamaStatus>, ApiError> {
    Ok(Json(state.services.hailo_ollama.status(&auth.actor).await?))
}

/// Start Hailo-Ollama (and again after a restart).
#[utoipa::path(post, path = "/api/v1/hw/ollama/start", operation_id = "start_hailo_ollama",
    tag = "hardware",
    responses((status = 200, body = HailoOllamaStatus), (status = 403), (status = 409), (status = 503)),
    security(("bearer_auth" = [])))]
pub(crate) async fn start_hailo_ollama(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<HailoOllamaStatus>, ApiError> {
    Ok(Json(state.services.hailo_ollama.start(&auth.actor).await?))
}

/// Stop the Hailo-Ollama MBL started.
#[utoipa::path(post, path = "/api/v1/hw/ollama/stop", operation_id = "stop_hailo_ollama",
    tag = "hardware", responses((status = 200, body = HailoOllamaStatus), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn stop_hailo_ollama(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<HailoOllamaStatus>, ApiError> {
    Ok(Json(state.services.hailo_ollama.stop(&auth.actor).await?))
}

/// Start downloading a Hailo-Ollama model (e.g. `qwen2.5:1.5b`).
#[utoipa::path(post, path = "/api/v1/hw/ollama/models/{name}/pull",
    operation_id = "pull_hailo_ollama_model", tag = "hardware", params(("name" = String, Path)),
    responses((status = 202), (status = 403), (status = 409), (status = 422)),
    security(("bearer_auth" = [])))]
pub(crate) async fn pull_hailo_ollama_model(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.hailo_ollama.pull(&auth.actor, &name).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Remove a Hailo-Ollama model.
#[utoipa::path(delete, path = "/api/v1/hw/ollama/models/{name}",
    operation_id = "delete_hailo_ollama_model", tag = "hardware", params(("name" = String, Path)),
    responses((status = 204), (status = 403), (status = 404)),
    security(("bearer_auth" = [])))]
pub(crate) async fn delete_hailo_ollama_model(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.hailo_ollama.delete(&auth.actor, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}
