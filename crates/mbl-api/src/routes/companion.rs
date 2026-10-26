//! AI Watch Companion endpoints (M7.16).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_service::{CompanionReaction, LiveSessionContext};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinSessionRequest {
    pub bot_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendReactionRequest {
    pub bot_name: String,
    pub timestamp_ms: i64,
    pub commentary: String,
    pub sentiment: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/companion/sessions/{id}/join", post(join_companion_session))
        .route("/api/v1/companion/sessions/{id}/reaction", post(send_companion_reaction))
        .route("/api/v1/companion/sessions/{id}/context", get(get_companion_context))
}

#[utoipa::path(
    post,
    path = "/api/v1/companion/sessions/{id}/join",
    params(
        ("id" = String, Path, description = "Live playback session ID")
    ),
    request_body = JoinSessionRequest,
    responses(
        (status = 204, description = "Companion bot joined session"),
    ),
    security(("bearer_auth" = []))
)]
async fn join_companion_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<JoinSessionRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.companion.join_session(&auth.actor, &id, &body.bot_name)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/companion/sessions/{id}/reaction",
    params(
        ("id" = String, Path, description = "Live playback session ID")
    ),
    request_body = SendReactionRequest,
    responses(
        (status = 200, description = "Companion reaction posted", body = CompanionReaction),
    ),
    security(("bearer_auth" = []))
)]
async fn send_companion_reaction(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SendReactionRequest>,
) -> Result<Json<CompanionReaction>, ApiError> {
    let reaction = state.services.companion.send_reaction(
        &auth.actor,
        &id,
        &body.bot_name,
        body.timestamp_ms,
        &body.commentary,
        body.sentiment.as_deref(),
    )?;
    Ok(Json(reaction))
}

#[utoipa::path(
    get,
    path = "/api/v1/companion/sessions/{id}/context",
    params(
        ("id" = String, Path, description = "Live playback session ID")
    ),
    responses(
        (status = 200, description = "Live playback context", body = LiveSessionContext),
        (status = 404, description = "Session not found")
    ),
    security(("bearer_auth" = []))
)]
async fn get_companion_context(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LiveSessionContext>, ApiError> {
    let ctx = state.services.companion.get_live_context(&auth.actor, &id)?;
    Ok(Json(ctx))
}
