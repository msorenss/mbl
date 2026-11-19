//! AI Watch Companion endpoints (M7.16).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use mbl_service::{
    CompanionFeed, CompanionFrequency, CompanionMode, CompanionReaction, CompanionWake,
    LiveSessionContext,
};
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
    /// ID of the viewer message this answers. Required while the viewer allows replies only.
    #[serde(default)]
    pub reply_to: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompanionModeRequest {
    pub mode: CompanionMode,
    /// How often companions comment on their own; unchanged when left out.
    #[serde(default)]
    pub frequency: Option<CompanionFrequency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ViewerMessageRequest {
    /// 1–500 characters.
    pub text: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/companion/sessions/{id}/join", post(join_companion_session))
        .route("/api/v1/companion/sessions/{id}/reaction", post(send_companion_reaction))
        .route("/api/v1/companion/sessions/{id}/context", get(get_companion_context))
        .route("/api/v1/companion/sessions/{id}/reactions", get(get_companion_reactions))
        .route("/api/v1/companion/sessions/{id}/mode", put(set_companion_mode))
        .route("/api/v1/companion/sessions/{id}/messages", post(post_viewer_message))
        .route("/api/v1/companion/sessions/{id}/nudge", post(request_companion_comment))
        .route("/api/v1/companion/sessions/{id}/wait", get(wait_for_companion_events))
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
        (status = 409, description = "The companion has not joined, or the viewer turned comments off or wants replies only"),
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
        body.reply_to,
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
    let ctx = state.services.companion.live_context(&auth.actor, &id).await?;
    Ok(Json(ctx))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReactionsQuery {
    /// Only reactions created after this time (ms since epoch).
    #[serde(default)]
    pub after: i64,
    /// Only entries with a larger ID; takes precedence over `after`.
    #[serde(default, rename = "afterId")]
    pub after_id: Option<u64>,
}

/// Connected companions and their latest reactions, for the player of this session.
#[utoipa::path(
    get,
    path = "/api/v1/companion/sessions/{id}/reactions",
    params(("id" = String, Path, description = "Live playback session ID"), ReactionsQuery),
    responses(
        (status = 200, description = "Companions and reactions", body = CompanionFeed),
        (status = 404, description = "Session not found")
    ),
    security(("bearer_auth" = []))
)]
async fn get_companion_reactions(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ReactionsQuery>,
) -> Result<Json<CompanionFeed>, ApiError> {
    let mut feed =
        state.services.companion.reactions_since(&auth.actor, &id, query.after, query.after_id)?;
    feed.assistant_available = state.services.assistant.is_enabled().await;
    Ok(Json(feed))
}

/// Choose what companions may post in this session: everything, only answers, or nothing.
#[utoipa::path(
    put,
    path = "/api/v1/companion/sessions/{id}/mode",
    params(("id" = String, Path, description = "Live playback session ID")),
    request_body = CompanionModeRequest,
    responses(
        (status = 204, description = "Mode saved"),
        (status = 403, description = "Only the person watching can change the mode"),
        (status = 404, description = "Session not found")
    ),
    security(("bearer_auth" = []))
)]
async fn set_companion_mode(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CompanionModeRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.companion.set_mode(&auth.actor, &id, body.mode, body.frequency)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Write to the companions watching with you. The message appears in the session feed.
#[utoipa::path(
    post,
    path = "/api/v1/companion/sessions/{id}/messages",
    params(("id" = String, Path, description = "Live playback session ID")),
    request_body = ViewerMessageRequest,
    responses(
        (status = 200, description = "Message posted", body = CompanionReaction),
        (status = 403, description = "Only the person watching can write"),
        (status = 404, description = "Session not found"),
        (status = 422, description = "Empty or too long message")
    ),
    security(("bearer_auth" = []))
)]
async fn post_viewer_message(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ViewerMessageRequest>,
) -> Result<Json<CompanionReaction>, ApiError> {
    Ok(Json(state.services.companion.post_viewer_message(&auth.actor, &id, &body.text)?))
}

/// Ask the connected companions for a comment right now.
#[utoipa::path(
    post,
    path = "/api/v1/companion/sessions/{id}/nudge",
    params(("id" = String, Path, description = "Live playback session ID")),
    responses(
        (status = 204, description = "Companions are asked to comment"),
        (status = 403, description = "Only the person watching can ask"),
        (status = 404, description = "Session not found"),
        (status = 409, description = "Comments are turned off")
    ),
    security(("bearer_auth" = []))
)]
async fn request_companion_comment(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.companion.request_comment(&auth.actor, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WaitQuery {
    /// Name the companion joined with.
    #[serde(rename = "botName")]
    pub bot_name: String,
    /// Only viewer messages with a larger ID (use `lastId` from the previous answer).
    #[serde(default, rename = "afterId")]
    pub after_id: Option<u64>,
    /// How long to wait, at most 25000 ms (default 25000).
    #[serde(default, rename = "timeoutMs")]
    pub timeout_ms: Option<u64>,
}

/// For companions: wait until the viewer writes, asks for a comment, or a scheduled comment
/// is due. Returns at once when something is already waiting.
#[utoipa::path(
    get,
    path = "/api/v1/companion/sessions/{id}/wait",
    params(("id" = String, Path, description = "Live playback session ID"), WaitQuery),
    responses(
        (status = 200, description = "Something to react to, or a timeout", body = CompanionWake),
        (status = 404, description = "Session not found or ended"),
        (status = 409, description = "The companion has not joined the session")
    ),
    security(("bearer_auth" = []))
)]
async fn wait_for_companion_events(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<WaitQuery>,
) -> Result<Json<CompanionWake>, ApiError> {
    Ok(Json(
        state
            .services
            .companion
            .wait_for_events(
                &auth.actor,
                &id,
                &query.bot_name,
                query.after_id,
                query.timeout_ms.unwrap_or(mbl_service::COMPANION_WAIT_MAX_MS),
            )
            .await?,
    ))
}
