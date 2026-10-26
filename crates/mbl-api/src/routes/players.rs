//! Players Registry & Companion Remote Control endpoints (M7.10, M7.13).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_service::{PlayerCommand, PlayerInfo};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/players", get(list_players).post(register_player))
        .route("/api/v1/players/{id}/command", post(send_player_command))
}

#[utoipa::path(
    get,
    path = "/api/v1/players",
    responses(
        (status = 200, description = "List reachable players", body = Vec<PlayerInfo>),
    ),
    security(("bearer_auth" = []))
)]
async fn list_players(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PlayerInfo>>, ApiError> {
    let list = state.services.players.list_players(&auth.actor)?;
    Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/v1/players",
    request_body = PlayerInfo,
    responses(
        (status = 204, description = "Player heartbeat/registration accepted"),
    ),
    security(("bearer_auth" = []))
)]
async fn register_player(
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<PlayerInfo>,
) -> Result<StatusCode, ApiError> {
    state.services.players.register_or_update(body);
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/players/{id}/command",
    params(
        ("id" = String, Path, description = "Target player ID")
    ),
    request_body = PlayerCommand,
    responses(
        (status = 204, description = "Player command dispatched"),
        (status = 404, description = "Player not found")
    ),
    security(("bearer_auth" = []))
)]
async fn send_player_command(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PlayerCommand>,
) -> Result<StatusCode, ApiError> {
    state.services.players.send_command(&auth.actor, &id, body)?;
    Ok(StatusCode::NO_CONTENT)
}
