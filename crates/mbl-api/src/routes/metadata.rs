//! Manual metadata editing for administrators.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::ItemId;
use mbl_service::metadata_edit::{
    ArtworkCandidates, ChooseArtwork, ItemMetadata, MatchItemRequest, UpdateItemMetadata,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::extract::AdminUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/items/{id}/metadata", get(get_metadata).patch(update_metadata))
        .route("/api/v1/items/{id}/match", post(match_item))
        .route("/api/v1/items/{id}/artwork", get(artwork_candidates).put(choose_artwork))
}

/// Editable metadata fields and which of them are locked.
#[utoipa::path(get, path = "/api/v1/items/{id}/metadata", tag = "metadata",
    params(("id" = i64, Path)),
    responses((status = 200, body = ItemMetadata), (status = 403), (status = 404)))]
pub(crate) async fn get_metadata(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<ItemId>,
) -> Result<Json<ItemMetadata>, ApiError> {
    Ok(Json(state.services.metadata.get(&auth.actor, id).await?))
}

/// Change fields. Every field sent is stored and locked against automatic matching.
#[utoipa::path(patch, path = "/api/v1/items/{id}/metadata", tag = "metadata",
    params(("id" = i64, Path)), request_body = UpdateItemMetadata,
    responses((status = 200, body = ItemMetadata), (status = 403), (status = 404), (status = 422)))]
pub(crate) async fn update_metadata(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<ItemId>,
    Json(body): Json<UpdateItemMetadata>,
) -> Result<Json<ItemMetadata>, ApiError> {
    Ok(Json(state.services.metadata.update(&auth.actor, id, body).await?))
}

/// Match a movie or show to another TMDB entry. Runs as a job; locked fields are kept.
#[utoipa::path(post, path = "/api/v1/items/{id}/match", tag = "metadata",
    params(("id" = i64, Path)), request_body = MatchItemRequest,
    responses((status = 202, description = "Match queued"), (status = 403), (status = 404), (status = 422)))]
pub(crate) async fn match_item(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<ItemId>,
    Json(body): Json<MatchItemRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.metadata.rematch(&auth.actor, id, body.tmdb_id).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ArtworkParams {
    /// `poster` (default) or `backdrop`.
    pub kind: Option<String>,
}

/// Poster or backdrop choices from TMDB.
#[utoipa::path(get, path = "/api/v1/items/{id}/artwork", tag = "metadata",
    params(("id" = i64, Path), ArtworkParams),
    responses((status = 200, body = ArtworkCandidates), (status = 403), (status = 404), (status = 503)))]
pub(crate) async fn artwork_candidates(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<ItemId>,
    Query(q): Query<ArtworkParams>,
) -> Result<Json<ArtworkCandidates>, ApiError> {
    let kind = q.kind.as_deref().unwrap_or("poster");
    Ok(Json(state.services.metadata.artwork_candidates(&auth.actor, id, kind).await?))
}

/// Use an image as poster or backdrop; it is downloaded in the background and locked.
#[utoipa::path(put, path = "/api/v1/items/{id}/artwork", tag = "metadata",
    params(("id" = i64, Path)), request_body = ChooseArtwork,
    responses((status = 204), (status = 403), (status = 404), (status = 422)))]
pub(crate) async fn choose_artwork(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<ItemId>,
    Json(body): Json<ChooseArtwork>,
) -> Result<StatusCode, ApiError> {
    state.services.metadata.choose_artwork(&auth.actor, id, body).await?;
    Ok(StatusCode::NO_CONTENT)
}
