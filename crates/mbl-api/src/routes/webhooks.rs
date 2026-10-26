//! Webhooks REST endpoints (M7.11).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use mbl_core::WebhookId;
use mbl_service::{CreateWebhookRequest, WebhookRow};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWebhookStateRequest {
    pub enabled: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/webhooks", get(list_webhooks).post(create_webhook))
        .route("/api/v1/webhooks/{id}", patch(toggle_webhook).delete(delete_webhook))
}

#[utoipa::path(
    get,
    path = "/api/v1/webhooks",
    responses(
        (status = 200, description = "List configured webhooks", body = Vec<WebhookRow>),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn list_webhooks(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<WebhookRow>>, ApiError> {
    let list = state.services.webhooks.list_webhooks(&auth.actor).await?;
    Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/v1/webhooks",
    request_body = CreateWebhookRequest,
    responses(
        (status = 201, description = "Webhook registered", body = WebhookRow),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn create_webhook(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookRow>), ApiError> {
    let row = state.services.webhooks.create_webhook(&auth.actor, body).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/webhooks/{id}",
    params(
        ("id" = i64, Path, description = "Webhook ID")
    ),
    request_body = UpdateWebhookStateRequest,
    responses(
        (status = 204, description = "Webhook enabled state updated"),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn toggle_webhook(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateWebhookStateRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.webhooks.set_enabled(&auth.actor, WebhookId(id), body.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/webhooks/{id}",
    params(
        ("id" = i64, Path, description = "Webhook ID")
    ),
    responses(
        (status = 204, description = "Webhook deleted"),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn delete_webhook(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.services.webhooks.delete_webhook(&auth.actor, WebhookId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
