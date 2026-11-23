//! Linked MBL servers: their players, remote control and playing from their libraries.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mbl_service::linked_servers::{
    AddLinkedServerRequest, LinkedItem, LinkedPlayRequest, LinkedPlayerCommand, LinkedServer,
    LinkedServerView,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/system/linked-servers", get(list_linked_servers).post(add_linked_server))
        .route("/api/v1/system/linked-servers/{id}", delete(remove_linked_server))
        .route("/api/v1/linked-servers", get(linked_server_overview))
        .route("/api/v1/linked-servers/names", get(linked_server_names))
        .route("/api/v1/linked-servers/{id}/search", get(search_linked_server))
        .route("/api/v1/linked-servers/{id}/players/{player}/command", post(command_linked_player))
        .route("/api/v1/linked-servers/{id}/players/{player}/play", post(play_on_linked_player))
}

/// Linked servers (admin). Tokens are never returned.
#[utoipa::path(get, path = "/api/v1/system/linked-servers", tag = "linked-servers",
    responses((status = 200, body = [LinkedServer]), (status = 403)),
    security(("bearer_auth" = [])))]
pub(crate) async fn list_linked_servers(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<LinkedServer>>, ApiError> {
    Ok(Json(state.services.linked_servers.list(&auth.actor).await?))
}

/// Link another MBL (admin). The token is checked against it first.
#[utoipa::path(post, path = "/api/v1/system/linked-servers", tag = "linked-servers",
    request_body = AddLinkedServerRequest,
    responses((status = 200, body = LinkedServer), (status = 403), (status = 409), (status = 422)),
    security(("bearer_auth" = [])))]
pub(crate) async fn add_linked_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<AddLinkedServerRequest>,
) -> Result<Json<LinkedServer>, ApiError> {
    Ok(Json(state.services.linked_servers.add(&auth.actor, body).await?))
}

/// Remove a link (admin). Revoke the token on the other server as well.
#[utoipa::path(delete, path = "/api/v1/system/linked-servers/{id}", tag = "linked-servers",
    params(("id" = String, Path)),
    responses((status = 204), (status = 403), (status = 404)),
    security(("bearer_auth" = [])))]
pub(crate) async fn remove_linked_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.linked_servers.remove(&auth.actor, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Linked servers you may use, with their players right now.
#[utoipa::path(get, path = "/api/v1/linked-servers", tag = "linked-servers",
    responses((status = 200, body = [LinkedServerView])),
    security(("bearer_auth" = [])))]
pub(crate) async fn linked_server_overview(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<LinkedServerView>>, ApiError> {
    Ok(Json(state.services.linked_servers.overview(&auth.actor).await?))
}

/// Linked servers you may use, by name and address only (for switching servers).
#[utoipa::path(get, path = "/api/v1/linked-servers/names", tag = "linked-servers",
    responses((status = 200, body = [LinkedServer])),
    security(("bearer_auth" = [])))]
pub(crate) async fn linked_server_names(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<LinkedServer>>, ApiError> {
    Ok(Json(state.services.linked_servers.names(&auth.actor).await?))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LinkedSearchQuery {
    pub q: String,
}

/// Search a linked server's library.
#[utoipa::path(get, path = "/api/v1/linked-servers/{id}/search", tag = "linked-servers",
    params(("id" = String, Path), LinkedSearchQuery),
    responses((status = 200, body = [LinkedItem]), (status = 404), (status = 503)),
    security(("bearer_auth" = [])))]
pub(crate) async fn search_linked_server(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LinkedSearchQuery>,
) -> Result<Json<Vec<LinkedItem>>, ApiError> {
    Ok(Json(state.services.linked_servers.search(&auth.actor, &id, &query.q).await?))
}

/// Pause, resume, stop, seek or set the volume on a linked server's player.
#[utoipa::path(post, path = "/api/v1/linked-servers/{id}/players/{player}/command",
    tag = "linked-servers",
    params(("id" = String, Path), ("player" = String, Path)),
    request_body = LinkedPlayerCommand,
    responses((status = 204), (status = 404), (status = 422), (status = 503)),
    security(("bearer_auth" = [])))]
pub(crate) async fn command_linked_player(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, player)): Path<(String, String)>,
    Json(body): Json<LinkedPlayerCommand>,
) -> Result<StatusCode, ApiError> {
    state.services.linked_servers.command(&auth.actor, &id, &player, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Play a title from the linked server's library on one of its players.
#[utoipa::path(post, path = "/api/v1/linked-servers/{id}/players/{player}/play",
    tag = "linked-servers",
    params(("id" = String, Path), ("player" = String, Path)),
    request_body = LinkedPlayRequest,
    responses((status = 204), (status = 404), (status = 503)),
    security(("bearer_auth" = [])))]
pub(crate) async fn play_on_linked_player(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, player)): Path<(String, String)>,
    Json(body): Json<LinkedPlayRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.linked_servers.play(&auth.actor, &id, &player, body).await?;
    Ok(StatusCode::NO_CONTENT)
}
