//! `GET /api/v1/events`: WebSocket push of bus events the user may see.

use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use mbl_core::Actor;
use tokio::sync::broadcast::error::RecvError;

use crate::AppState;
use crate::extract::AuthUser;

const PING_INTERVAL: Duration = Duration::from_secs(30);

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/events", get(events))
}

/// WebSocket stream of `{type, data}` events the user may see.
#[utoipa::path(get, path = "/api/v1/events", tag = "events",
    responses((status = 101, description = "Switching to WebSocket")))]
async fn events(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| forward(state, auth.actor, socket))
}

async fn forward(state: AppState, actor: Actor, mut socket: WebSocket) {
    let events = &state.services.events;
    let mut rx = events.subscribe();
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.tick().await;
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(event) => {
                    if !events.visible(&actor, &event).await {
                        continue;
                    }
                    let Ok(text) = serde_json::to_string(&event) else { continue };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                Err(RecvError::Lagged(n)) => tracing::debug!(missed = n, "event subscriber lagged"),
                Err(RecvError::Closed) => return,
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return;
                }
            }
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
}
