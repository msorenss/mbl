//! What is missing from the library compared with TMDB.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use mbl_core::{ItemId, LibraryId};
use mbl_service::missing::{LibraryMissing, ShowMissing};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/items/{id}/missing", get(show_missing))
        .route("/api/v1/libraries/{id}/missing", get(library_missing))
}

/// Missing aired episodes of a show: gaps, episodes after the last one you have and whole
/// seasons (specials are left out), with titles from TMDB.
#[utoipa::path(get, path = "/api/v1/items/{id}/missing", operation_id = "show_missing", tag = "browse",
    params(("id" = i64, Path, description = "Show")),
    responses((status = 200, body = ShowMissing), (status = 404),
        (status = 422, description = "Not a show, or no TMDB API key"),
        (status = 503, description = "TMDB did not answer")),
    security(("bearer_auth" = [])))]
pub(crate) async fn show_missing(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<ItemId>,
) -> Result<Json<ShowMissing>, ApiError> {
    Ok(Json(state.services.missing.show(&auth.actor, id).await?))
}

/// What a TV or movie library lacks: shows with missing episodes, and released movies from
/// collections (for example the second part of a trilogy).
#[utoipa::path(get, path = "/api/v1/libraries/{id}/missing", operation_id = "library_missing", tag = "browse",
    params(("id" = i64, Path, description = "Library")),
    responses((status = 200, body = LibraryMissing), (status = 404),
        (status = 422, description = "No TMDB API key"),
        (status = 503, description = "TMDB did not answer")),
    security(("bearer_auth" = [])))]
pub(crate) async fn library_missing(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<LibraryId>,
) -> Result<Json<LibraryMissing>, ApiError> {
    Ok(Json(state.services.missing.library(&auth.actor, id).await?))
}
