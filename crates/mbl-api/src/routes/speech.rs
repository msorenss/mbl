//! Speech to text on the server (Whisper on a Hailo-10H).

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_service::speech::{MAX_AUDIO_BYTES, SpeechStatus, Transcript};
use serde::Deserialize;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/speech/status", get(speech_status)).route(
        "/api/v1/speech/transcribe",
        post(transcribe).layer(DefaultBodyLimit::max(MAX_AUDIO_BYTES)),
    )
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TranscribeQuery {
    /// Two-letter language code; leave out to let the model decide.
    pub language: Option<String>,
}

/// Whether the server can turn speech into text.
#[utoipa::path(get, path = "/api/v1/speech/status", operation_id = "speech_status", tag = "speech",
    responses((status = 200, body = SpeechStatus)),
    security(("bearer_auth" = [])))]
pub(crate) async fn speech_status(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Json<SpeechStatus> {
    Json(state.services.speech.status().await)
}

/// Turn a short recording (the request body, e.g. `audio/webm`, up to 4 MB) into text.
#[utoipa::path(post, path = "/api/v1/speech/transcribe", operation_id = "transcribe_speech", tag = "speech",
    params(TranscribeQuery),
    request_body(content = Vec<u8>, content_type = "audio/webm", description = "The recording"),
    responses((status = 200, body = Transcript), (status = 403), (status = 422, description = "Unreadable or too long"),
        (status = 503, description = "No speech recognition on this server")),
    security(("bearer_auth" = [])))]
pub(crate) async fn transcribe(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<TranscribeQuery>,
    body: Bytes,
) -> Result<Json<Transcript>, ApiError> {
    Ok(Json(state.services.speech.transcribe(&auth.actor, &body, q.language.as_deref()).await?))
}
