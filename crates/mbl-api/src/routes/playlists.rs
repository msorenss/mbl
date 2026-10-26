//! Playlists REST endpoints (M7.6).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::{ItemId, PlaylistId};
use mbl_service::{CreatePlaylistRequest, PlaylistRow, UpdatePlaylistRequest};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/playlists", get(list_playlists).post(create_playlist))
        .route(
            "/api/v1/playlists/{id}",
            get(get_playlist).patch(update_playlist).delete(delete_playlist),
        )
        .route("/api/v1/playlists/{id}/items", get(get_playlist_items))
        .route(
            "/api/v1/playlists/{id}/items/{itemId}",
            post(add_item_to_playlist).delete(remove_item_from_playlist),
        )
}

#[utoipa::path(
    get,
    path = "/api/v1/playlists",
    responses(
        (status = 200, description = "List playlists for acting user plus public playlists", body = Vec<PlaylistRow>),
    ),
    security(("bearer_auth" = []))
)]
async fn list_playlists(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PlaylistRow>>, ApiError> {
    let list = state.services.playlists.list_playlists(&auth.actor).await?;
    Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/v1/playlists",
    request_body = CreatePlaylistRequest,
    responses(
        (status = 201, description = "Playlist created", body = PlaylistRow),
    ),
    security(("bearer_auth" = []))
)]
async fn create_playlist(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreatePlaylistRequest>,
) -> Result<(StatusCode, Json<PlaylistRow>), ApiError> {
    let row = state.services.playlists.create_playlist(&auth.actor, body).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    get,
    path = "/api/v1/playlists/{id}",
    params(
        ("id" = i64, Path, description = "Playlist ID")
    ),
    responses(
        (status = 200, description = "Playlist detail", body = PlaylistRow),
        (status = 404, description = "Playlist not found")
    ),
    security(("bearer_auth" = []))
)]
async fn get_playlist(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<PlaylistRow>, ApiError> {
    let row = state
        .services
        .playlists
        .get_playlist(&auth.actor, PlaylistId(id))
        .await?
        .ok_or(mbl_core::Error::NotFound)?;
    Ok(Json(row))
}

#[utoipa::path(
    patch,
    path = "/api/v1/playlists/{id}",
    params(
        ("id" = i64, Path, description = "Playlist ID")
    ),
    request_body = UpdatePlaylistRequest,
    responses(
        (status = 204, description = "Playlist updated"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Playlist not found")
    ),
    security(("bearer_auth" = []))
)]
async fn update_playlist(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdatePlaylistRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.playlists.update_playlist(&auth.actor, PlaylistId(id), body).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/playlists/{id}",
    params(
        ("id" = i64, Path, description = "Playlist ID")
    ),
    responses(
        (status = 204, description = "Playlist deleted"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Playlist not found")
    ),
    security(("bearer_auth" = []))
)]
async fn delete_playlist(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.services.playlists.delete_playlist(&auth.actor, PlaylistId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/playlists/{id}/items",
    params(
        ("id" = i64, Path, description = "Playlist ID")
    ),
    responses(
        (status = 200, description = "Item IDs in playlist", body = Vec<i64>),
    ),
    security(("bearer_auth" = []))
)]
async fn get_playlist_items(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<i64>>, ApiError> {
    let items = state.services.playlists.get_items(&auth.actor, PlaylistId(id)).await?;
    Ok(Json(items.into_iter().map(|it| it.0).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/playlists/{id}/items/{itemId}",
    params(
        ("id" = i64, Path, description = "Playlist ID"),
        ("itemId" = i64, Path, description = "Media Item ID")
    ),
    responses(
        (status = 204, description = "Item added to playlist"),
    ),
    security(("bearer_auth" = []))
)]
async fn add_item_to_playlist(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, item_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    state.services.playlists.add_item(&auth.actor, PlaylistId(id), ItemId(item_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/playlists/{id}/items/{itemId}",
    params(
        ("id" = i64, Path, description = "Playlist ID"),
        ("itemId" = i64, Path, description = "Media Item ID")
    ),
    responses(
        (status = 204, description = "Item removed from playlist"),
    ),
    security(("bearer_auth" = []))
)]
async fn remove_item_from_playlist(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, item_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    state.services.playlists.remove_item(&auth.actor, PlaylistId(id), ItemId(item_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
