//! REST endpoints for watch history and library import from Plex and Jellyfin (M10.5).

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use mbl_service::{ImportSummary, JellyfinImportRequest, PlexImportRequest};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/import/plex", post(import_plex))
        .route("/api/v1/import/jellyfin", post(import_jellyfin))
}

/// Import Plex watch history and view state into the authenticated user's account.
#[utoipa::path(
    post,
    path = "/api/v1/import/plex",
    tag = "Import",
    security(("bearer_auth" = []), ("session_cookie" = [])),
    request_body = PlexImportRequest,
    responses(
        (status = 200, description = "Plex watch history successfully imported", body = ImportSummary),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn import_plex(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<PlexImportRequest>,
) -> Result<Json<ImportSummary>, ApiError> {
    let summary = state.services.import.import_plex(&auth.actor, req).await?;
    Ok(Json(summary))
}

/// Import Jellyfin watch history and playback state into the authenticated user's account.
#[utoipa::path(
    post,
    path = "/api/v1/import/jellyfin",
    tag = "Import",
    security(("bearer_auth" = []), ("session_cookie" = [])),
    request_body = JellyfinImportRequest,
    responses(
        (status = 200, description = "Jellyfin watch history successfully imported", body = ImportSummary),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn import_jellyfin(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<JellyfinImportRequest>,
) -> Result<Json<ImportSummary>, ApiError> {
    let summary = state.services.import.import_jellyfin(&auth.actor, req).await?;
    Ok(Json(summary))
}
