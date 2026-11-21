//! Find and download subtitles (OpenSubtitles).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::ItemId;
use mbl_service::subtitle_search::{
    DownloadSubtitleRequest, DownloadedSubtitleTrack, OpenSubtitlesSettings, SubtitleSearchHit,
    UpdateOpenSubtitlesSettings,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/system/opensubtitles", get(get_settings).put(update_settings))
        .route("/api/v1/system/opensubtitles/test", post(test_settings))
        .route("/api/v1/subtitles/status", get(status))
        .route("/api/v1/items/{id}/subtitles/search", get(search))
        .route("/api/v1/items/{id}/subtitles/download", post(download))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubtitleSearchQuery {
    /// Two-letter language code, e.g. `sv` or `en`.
    pub language: String,
}

/// Whether subtitle search is set up.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSearchStatus {
    pub available: bool,
}

#[utoipa::path(get, path = "/api/v1/system/opensubtitles", operation_id = "get_opensubtitles_settings", tag = "subtitles",
    responses((status = 200, body = OpenSubtitlesSettings), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn get_settings(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<OpenSubtitlesSettings>, ApiError> {
    Ok(Json(state.services.subtitles.get_settings(&auth.actor).await?))
}

#[utoipa::path(put, path = "/api/v1/system/opensubtitles", operation_id = "update_opensubtitles_settings", tag = "subtitles", request_body = UpdateOpenSubtitlesSettings,
    responses((status = 200, body = OpenSubtitlesSettings), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn update_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateOpenSubtitlesSettings>,
) -> Result<Json<OpenSubtitlesSettings>, ApiError> {
    Ok(Json(state.services.subtitles.update_settings(&auth.actor, body).await?))
}

/// Check the API key, and the sign-in when a username is stored.
#[utoipa::path(post, path = "/api/v1/system/opensubtitles/test", operation_id = "test_opensubtitles_settings", tag = "subtitles",
    responses((status = 204, description = "OpenSubtitles accepted the key"), (status = 403),
        (status = 422, description = "No API key"), (status = 503, description = "Rejected or unreachable, with the reason")),
    security(("bearer_auth" = [])))]
pub(crate) async fn test_settings(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    state.services.subtitles.test(&auth.actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/v1/subtitles/status", operation_id = "subtitle_search_status", tag = "subtitles",
    responses((status = 200, body = SubtitleSearchStatus)),
    security(("bearer_auth" = [])))]
pub(crate) async fn status(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<SubtitleSearchStatus>, ApiError> {
    Ok(Json(SubtitleSearchStatus { available: state.services.subtitles.available().await? }))
}

/// Subtitles for a movie or episode, most downloaded first.
#[utoipa::path(get, path = "/api/v1/items/{id}/subtitles/search", operation_id = "search_subtitles", tag = "subtitles",
    params(("id" = i64, Path), SubtitleSearchQuery),
    responses((status = 200, body = Vec<SubtitleSearchHit>), (status = 403), (status = 404),
        (status = 422, description = "Not set up, bad language or not a movie/episode"),
        (status = 503, description = "OpenSubtitles refused or did not answer")),
    security(("bearer_auth" = [])))]
pub(crate) async fn search(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<ItemId>,
    Query(q): Query<SubtitleSearchQuery>,
) -> Result<Json<Vec<SubtitleSearchHit>>, ApiError> {
    Ok(Json(state.services.subtitles.search(&auth.actor, id, &q.language).await?))
}

/// Download a subtitle next to the video; it becomes a track at once.
#[utoipa::path(post, path = "/api/v1/items/{id}/subtitles/download", operation_id = "download_subtitle", tag = "subtitles",
    params(("id" = i64, Path)), request_body = DownloadSubtitleRequest,
    responses((status = 201, body = DownloadedSubtitleTrack), (status = 403), (status = 404),
        (status = 422, description = "Not set up, or MBL may not write in the folder"),
        (status = 503, description = "OpenSubtitles refused (for example the daily limit) or did not answer")),
    security(("bearer_auth" = [])))]
pub(crate) async fn download(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<ItemId>,
    Json(body): Json<DownloadSubtitleRequest>,
) -> Result<(StatusCode, Json<DownloadedSubtitleTrack>), ApiError> {
    let track = state.services.subtitles.download(&auth.actor, id, body).await?;
    Ok((StatusCode::CREATED, Json(track)))
}
