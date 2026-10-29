//! Players Registry & Companion Remote Control endpoints (M7.10, M7.13).

use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mbl_service::{PlayerCommand, PlayerInfo};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/players", get(list_players).post(register_player))
        .route("/api/v1/players/{id}", delete(unregister_player))
        .route("/api/v1/players/{id}/connect", get(connect_player))
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
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<PlayerInfo>,
) -> Result<StatusCode, ApiError> {
    if let Some(id) = body.now_playing_item_id {
        state.services.items.get(&auth.actor, id).await?;
    }
    state.services.players.register(&auth.actor, auth.session_id, body)?;
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
        (status = 404, description = "Player not found"),
        (status = 503, description = "Player command connection unavailable")
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

#[utoipa::path(
    delete,
    path = "/api/v1/players/{id}",
    params(
        ("id" = String, Path, description = "Target player ID")
    ),
    responses(
        (status = 204, description = "Player unregistered"),
    ),
    security(("bearer_auth" = []))
)]
async fn unregister_player(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.players.unregister_owned(&auth.actor, auth.session_id, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Session-bound command transport for an explicitly registered player.
#[utoipa::path(get, path = "/api/v1/players/{id}/connect", params(("id" = String, Path)),
    responses((status = 101, description = "Player command WebSocket"), (status = 404), (status = 409)))]
pub(crate) async fn connect_player(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let mut connection = state.services.players.connect(&auth.actor, auth.session_id, &id)?;
    Ok(ws.max_message_size(65_536).max_frame_size(65_536).on_upgrade(move |mut socket| async move {
        let mut ping = tokio::time::interval(std::time::Duration::from_secs(20));
        loop {
            tokio::select! {
                command = connection.commands.recv() => {
                    let Some(command) = command else { break; };
                    let Ok(json) = serde_json::to_string(&command) else { break; };
                    if socket.send(Message::Text(json.into())).await.is_err() { break; }
                },
                _ = ping.tick() => {
                    let active = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ? AND user_id = ? AND expires_at > ?)")
                        .bind(auth.session_id.0).bind(auth.actor.user_id.0).bind(mbl_core::time::now_ms())
                        .fetch_one(&state.db.read).await.unwrap_or(false);
                    if !active || socket.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
                },
                incoming = socket.recv() => match incoming { Some(Ok(Message::Close(_)) | Err(_)) | None => break, _ => {} },
            }
        }
    }))
}
