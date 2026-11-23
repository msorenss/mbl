//! AI Watch Companion endpoints (M7.16).

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use mbl_service::{
    CompanionFeed, CompanionFrames, CompanionFrequency, CompanionMode, CompanionReaction,
    CompanionWake, LiveSessionContext,
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
    /// Whether and how often a screenshot may go to the AI; unchanged when left out.
    #[serde(default)]
    pub frames: Option<CompanionFrames>,
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
        .route("/api/v1/companion/sessions/{id}/frame", get(companion_frame))
        .route("/api/v1/companion/sessions/{id}/receiver-token", post(companion_receiver_token))
        .merge(
            // The Cast receiver runs on another origin and has no MBL sign-in, only the token.
            Router::new()
                .route(
                    "/api/v1/companion/receiver/{id}/reactions",
                    get(receiver_reactions).options(crate::routes::stream::media_preflight),
                )
                .route(
                    "/api/v1/companion/receiver/{id}/speech/{reaction}",
                    get(receiver_speech).options(crate::routes::stream::media_preflight),
                )
                .layer(axum::middleware::map_response(crate::routes::stream::media_cors)),
        )
}

/// A token for the Cast receiver of this session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompanionReceiverToken {
    /// Lets the receiver read this session's comments and hear them read aloud, nothing else.
    pub token: String,
    pub expires_at: i64,
}

/// Let the TV read the companions' comments itself while casting.
#[utoipa::path(
    post,
    path = "/api/v1/companion/sessions/{id}/receiver-token",
    params(("id" = String, Path, description = "Live playback session ID")),
    responses(
        (status = 200, description = "Token for the receiver", body = CompanionReceiverToken),
        (status = 404, description = "Session not found")
    ),
    security(("bearer_auth" = []))
)]
async fn companion_receiver_token(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CompanionReceiverToken>, ApiError> {
    // Only someone who can see the session gets a token for it.
    state.services.companion.get_live_context(&auth.actor, &id)?;
    let (token, expires_at) = state.services.auth.sign_companion_receiver_token(&auth.actor, &id);
    Ok(Json(CompanionReceiverToken { token, expires_at }))
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReceiverQuery {
    /// Token from `receiver-token`.
    pub token: String,
    /// Only entries with a larger ID.
    #[serde(default, rename = "afterId")]
    pub after_id: Option<u64>,
    /// Only entries created after this time (ms since epoch), when `afterId` is left out.
    #[serde(default)]
    pub after: i64,
    /// Voice language for reading aloud, e.g. `sv`.
    #[serde(default)]
    pub language: Option<String>,
}

/// The session's comments, for the Cast receiver (token instead of sign-in).
#[utoipa::path(
    get,
    path = "/api/v1/companion/receiver/{id}/reactions",
    params(("id" = String, Path, description = "Live playback session ID"), ReceiverQuery),
    responses(
        (status = 200, description = "Comments", body = CompanionFeed),
        (status = 401, description = "Missing, wrong or expired token"),
        (status = 404, description = "Session not found")
    )
)]
async fn receiver_reactions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ReceiverQuery>,
) -> Result<Json<CompanionFeed>, ApiError> {
    let actor =
        state.services.auth.authenticate_companion_receiver_token(&query.token, &id).await?;
    Ok(Json(state.services.companion.reactions_since(&actor, &id, query.after, query.after_id)?))
}

/// One companion comment read aloud on the server (WAV), for the Cast receiver.
#[utoipa::path(
    get,
    path = "/api/v1/companion/receiver/{id}/speech/{reaction}",
    params(
        ("id" = String, Path, description = "Live playback session ID"),
        ("reaction" = u64, Path, description = "Comment ID"),
        ReceiverQuery
    ),
    responses(
        (status = 200, description = "WAV audio", content_type = "audio/wav"),
        (status = 401, description = "Missing, wrong or expired token"),
        (status = 404, description = "No such comment"),
        (status = 503, description = "No server voice for the language")
    )
)]
async fn receiver_speech(
    State(state): State<AppState>,
    Path((id, reaction)): Path<(String, u64)>,
    Query(query): Query<ReceiverQuery>,
) -> Result<Response, ApiError> {
    let actor =
        state.services.auth.authenticate_companion_receiver_token(&query.token, &id).await?;
    let text = state.services.companion.comment_text(&actor, &id, reaction)?;
    let language = query.language.as_deref().unwrap_or("en");
    let wav = state.services.voices.speak(&actor, &text, language).await?;
    Ok(([(header::CONTENT_TYPE, "audio/wav"), (header::CACHE_CONTROL, "no-store")], wav)
        .into_response())
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
    state.services.companion.set_mode(&auth.actor, &id, body.mode, body.frequency, body.frames)?;
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

/// A screenshot of the scene at the viewer's position (JPEG, at most 768 px wide), for an AI.
/// Only when the viewer has turned screenshots on; never from later than the position.
#[utoipa::path(
    get,
    path = "/api/v1/companion/sessions/{id}/frame",
    params(("id" = String, Path, description = "Live playback session ID")),
    responses(
        (status = 200, description = "Screenshot; X-Mbl-Frame-Position gives its position in ms", content_type = "image/jpeg"),
        (status = 404, description = "Session not found"),
        (status = 409, description = "Screenshots are off for this session"),
        (status = 503, description = "Another screenshot is being taken, or the video cannot be read")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn companion_frame(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    use axum::http::header;
    use axum::response::IntoResponse;
    let frame = state.services.companion.frame(&auth.actor, &id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/jpeg".to_owned()),
            (header::CACHE_CONTROL, "private, no-store".to_owned()),
            (
                header::HeaderName::from_static("x-mbl-frame-position"),
                frame.position_ms.to_string(),
            ),
        ],
        frame.jpeg.as_ref().clone(),
    )
        .into_response())
}
