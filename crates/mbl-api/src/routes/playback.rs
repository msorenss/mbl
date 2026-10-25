//! Playback session negotiation, progress reporting, and stop endpoints.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use mbl_core::Error;
use mbl_service::{
    PlaybackProgressRequest, PlaybackRequest, PlaybackResponse, PlaybackStopRequest,
};

use crate::extract::AuthUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/playback", post(start))
        .route("/api/v1/playback/{session}/progress", post(progress))
        .route("/api/v1/playback/{session}/stop", post(stop))
}

/// Start a playback session for a media item (`decide` -> DirectPlay or fMP4 HLS).
#[utoipa::path(
    post,
    path = "/api/v1/playback",
    tag = "playback",
    request_body = PlaybackRequest,
    responses(
        (status = 200, body = PlaybackResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Item or part not found"),
        (status = 422, description = "Invalid playback request"),
    )
)]
pub async fn start(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<PlaybackRequest>,
) -> Result<Json<PlaybackResponse>, ApiError> {
    let res = state.services.playback.start(&auth.actor, req).await?;
    Ok(Json(res))
}

/// Report playback progress (`playing` | `paused` | `buffering`) every ~10 seconds.
#[utoipa::path(
    post,
    path = "/api/v1/playback/{session}/progress",
    tag = "playback",
    params(("session" = String, Path)),
    request_body = PlaybackProgressRequest,
    responses(
        (status = 204, description = "Progress recorded"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Session not found"),
    )
)]
pub async fn progress(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(session): Path<String>,
    Json(req): Json<PlaybackProgressRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.playback.progress(&auth.actor, &session, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stop an active playback session, recording `watch_history` and tearing down any HLS session.
#[utoipa::path(
    post,
    path = "/api/v1/playback/{session}/stop",
    tag = "playback",
    params(("session" = String, Path)),
    request_body(content = Option<PlaybackStopRequest>, content_type = "application/json"),
    responses(
        (status = 204, description = "Session stopped"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Session not found"),
    )
)]
pub async fn stop(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(session): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let req = if body.iter().all(u8::is_ascii_whitespace) {
        PlaybackStopRequest::default()
    } else {
        serde_json::from_slice::<PlaybackStopRequest>(&body)
            .map_err(|e| Error::Validation(e.to_string()))?
    };
    state.services.playback.stop(&auth.actor, &session, req).await?;
    Ok(StatusCode::NO_CONTENT)
}
