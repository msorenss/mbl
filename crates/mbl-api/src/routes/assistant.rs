//! Built-in assistant settings: language model provider, connection test and model list.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_agent::ConnectionReport;
use mbl_service::assistant::{AssistantConnection, AssistantSettings, UpdateAssistantSettings};

use crate::error::ApiError;
use crate::extract::AdminUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/system/assistant",
            get(get_assistant_settings).put(update_assistant_settings),
        )
        .route("/api/v1/system/assistant/test", post(test_assistant_connection))
        .route("/api/v1/system/assistant/models", post(list_assistant_models))
}

/// Which language model the built-in assistant uses. The API key is never returned.
#[utoipa::path(
    get,
    path = "/api/v1/system/assistant",
    responses((status = 200, description = "Assistant settings", body = AssistantSettings)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn get_assistant_settings(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<AssistantSettings>, ApiError> {
    Ok(Json(state.services.assistant.get_settings(&auth.actor).await?))
}

/// Save the provider, address, key and model. Turning it on needs an address and a model.
#[utoipa::path(
    put,
    path = "/api/v1/system/assistant",
    request_body = UpdateAssistantSettings,
    responses(
        (status = 200, description = "Saved settings", body = AssistantSettings),
        (status = 422, description = "Invalid address, effort or missing model")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn update_assistant_settings(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateAssistantSettings>,
) -> Result<Json<AssistantSettings>, ApiError> {
    Ok(Json(state.services.assistant.update_settings(&auth.actor, body).await?))
}

/// Ask the model a short question and check that it can call tools. Problems are reported in
/// the body (`ok: false`), not as an error status.
#[utoipa::path(
    post,
    path = "/api/v1/system/assistant/test",
    request_body = AssistantConnection,
    responses(
        (status = 200, description = "Test result", body = ConnectionReport),
        (status = 422, description = "Invalid address or no model")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn test_assistant_connection(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<AssistantConnection>,
) -> Result<Json<ConnectionReport>, ApiError> {
    Ok(Json(state.services.assistant.test_connection(&auth.actor, body).await?))
}

/// Models the provider offers, for choosing one.
#[utoipa::path(
    post,
    path = "/api/v1/system/assistant/models",
    request_body = AssistantConnection,
    responses(
        (status = 200, description = "Model names", body = Vec<String>),
        (status = 503, description = "The provider could not be reached or rejected the key")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn list_assistant_models(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<AssistantConnection>,
) -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(state.services.assistant.list_models(&auth.actor, body).await?))
}
