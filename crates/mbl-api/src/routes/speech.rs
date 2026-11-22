//! Speech on the server: to text (Whisper on a Hailo-10H) and read aloud (Piper).

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_service::speech::{MAX_AUDIO_BYTES, SpeechStatus, Transcript};
use mbl_service::voices::VoicesStatus;
use serde::Deserialize;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/speech/status", get(speech_status))
        .route(
            "/api/v1/speech/transcribe",
            post(transcribe).layer(DefaultBodyLimit::max(MAX_AUDIO_BYTES)),
        )
        .route("/api/v1/speech/say", post(say))
        .route("/api/v1/speech/voices", get(voices_status))
        .route("/api/v1/speech/piper", post(install_piper))
        .route("/api/v1/speech/voices/{language}", post(install_voice).delete(remove_voice))
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
    let mut status = state.services.speech.status().await;
    status.server_voices = state.services.voices.languages().await;
    Json(status)
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SayRequest {
    /// Up to 2,000 characters.
    pub text: String,
    /// Two-letter language code.
    pub language: String,
}

/// Read text aloud on the server (Piper): a WAV file for the browser to play.
#[utoipa::path(post, path = "/api/v1/speech/say", operation_id = "say_text", tag = "speech",
    request_body = SayRequest,
    responses((status = 200, content_type = "audio/wav"), (status = 403), (status = 422),
        (status = 503, description = "No voice for this language on the server")),
    security(("bearer_auth" = [])))]
pub(crate) async fn say(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<SayRequest>,
) -> Result<Response, ApiError> {
    let wav = state.services.voices.speak(&auth.actor, &body.text, &body.language).await?;
    Ok(([(header::CONTENT_TYPE, "audio/wav"), (header::CACHE_CONTROL, "no-store")], wav)
        .into_response())
}

/// The Piper program and voices for reading aloud on the server (admin).
#[utoipa::path(get, path = "/api/v1/speech/voices", operation_id = "voices_status", tag = "speech",
    responses((status = 200, body = VoicesStatus), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn voices_status(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<VoicesStatus>, ApiError> {
    Ok(Json(state.services.voices.status(&auth.actor).await?))
}

/// Start downloading the Piper program (admin, about 25 MB).
#[utoipa::path(post, path = "/api/v1/speech/piper", operation_id = "install_piper", tag = "speech",
    responses((status = 202), (status = 403), (status = 409), (status = 422)),
    security(("bearer_auth" = [])))]
pub(crate) async fn install_piper(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    state.services.voices.install_program(&auth.actor).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Start downloading the voice for a language (admin, about 60 MB).
#[utoipa::path(post, path = "/api/v1/speech/voices/{language}", operation_id = "install_voice",
    tag = "speech", params(("language" = String, Path)),
    responses((status = 202), (status = 403), (status = 409), (status = 422)),
    security(("bearer_auth" = [])))]
pub(crate) async fn install_voice(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(language): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.voices.install_voice(&auth.actor, &language).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Remove an installed voice (admin).
#[utoipa::path(delete, path = "/api/v1/speech/voices/{language}", operation_id = "remove_voice",
    tag = "speech", params(("language" = String, Path)),
    responses((status = 204), (status = 403), (status = 404)),
    security(("bearer_auth" = [])))]
pub(crate) async fn remove_voice(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(language): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.voices.remove_voice(&auth.actor, &language).await?;
    Ok(StatusCode::NO_CONTENT)
}
