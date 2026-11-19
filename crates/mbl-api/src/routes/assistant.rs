//! Built-in assistant settings: language model provider, connection test and model list.

use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::Stream;
use mbl_agent::ConnectionReport;
use mbl_core::Error;
use mbl_db::repo::assistant::Conversation;
use mbl_service::assistant::{
    AssistantConnection, AssistantSettings, AssistantStatus, ConversationDetail,
    UpdateAssistantSettings,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::{AdminUser, AuthUser};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssistantChatRequest {
    /// Continue this conversation; leave out to start a new one.
    #[serde(default)]
    pub conversation_id: Option<i64>,
    /// 1–2000 characters.
    pub text: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/system/assistant",
            get(get_assistant_settings).put(update_assistant_settings),
        )
        .route("/api/v1/system/assistant/test", post(test_assistant_connection))
        .route("/api/v1/system/assistant/models", post(list_assistant_models))
        .route("/api/v1/assistant/status", get(assistant_status))
        .route("/api/v1/assistant/conversations", get(list_conversations))
        .route(
            "/api/v1/assistant/conversations/{id}",
            get(get_conversation).delete(delete_conversation),
        )
        .route("/api/v1/assistant/chat", post(assistant_chat))
}

/// Which language model the built-in assistant uses. The API key is never returned.
#[utoipa::path(
    get,
    path = "/api/v1/system/assistant",
    responses((status = 200, description = "Assistant settings", body = AssistantSettings)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn get_assistant_settings(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
) -> Result<Json<AssistantSettings>, ApiError> {
    Ok(Json(state.services.assistant.get_settings(&auth.actor).await?))
}

/// Save the provider, address, key and model. Turning it on needs an address and a model.
#[utoipa::path(
    put,
    path = "/api/v1/system/assistant",
    request_body = UpdateAssistantSettings,
    responses(
        (status = 200, description = "Saved settings", body = AssistantSettings),
        (status = 422, description = "Invalid address, effort or missing model")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn update_assistant_settings(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateAssistantSettings>,
) -> Result<Json<AssistantSettings>, ApiError> {
    Ok(Json(state.services.assistant.update_settings(&auth.actor, body).await?))
}

/// Ask the model a short question and check that it can call tools. Problems are reported in
/// the body (`ok: false`), not as an error status.
#[utoipa::path(
    post,
    path = "/api/v1/system/assistant/test",
    request_body = AssistantConnection,
    responses(
        (status = 200, description = "Test result", body = ConnectionReport),
        (status = 422, description = "Invalid address or no model")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn test_assistant_connection(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<AssistantConnection>,
) -> Result<Json<ConnectionReport>, ApiError> {
    Ok(Json(state.services.assistant.test_connection(&auth.actor, body).await?))
}

/// Models the provider offers, for choosing one.
#[utoipa::path(
    post,
    path = "/api/v1/system/assistant/models",
    request_body = AssistantConnection,
    responses(
        (status = 200, description = "Model names", body = Vec<String>),
        (status = 503, description = "The provider could not be reached or rejected the key")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn list_assistant_models(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<AssistantConnection>,
) -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(state.services.assistant.list_models(&auth.actor, body).await?))
}

/// Whether the assistant chat can be used by the person asking.
#[utoipa::path(
    get,
    path = "/api/v1/assistant/status",
    responses((status = 200, description = "Availability", body = AssistantStatus)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn assistant_status(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Json<AssistantStatus> {
    Json(state.services.assistant.status(&auth.actor).await)
}

/// The person's conversations with the assistant, most recent first.
#[utoipa::path(
    get,
    path = "/api/v1/assistant/conversations",
    responses((status = 200, description = "Conversations", body = Vec<Conversation>)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn list_conversations(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Conversation>>, ApiError> {
    Ok(Json(state.services.assistant.conversations(&auth.actor).await?))
}

/// One of the person's conversations with its messages.
#[utoipa::path(
    get,
    path = "/api/v1/assistant/conversations/{id}",
    params(("id" = i64, Path, description = "Conversation ID")),
    responses(
        (status = 200, description = "Conversation", body = ConversationDetail),
        (status = 404, description = "Not found")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn get_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ConversationDetail>, ApiError> {
    Ok(Json(state.services.assistant.conversation(&auth.actor, id).await?))
}

#[utoipa::path(
    delete,
    path = "/api/v1/assistant/conversations/{id}",
    params(("id" = i64, Path, description = "Conversation ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn delete_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.services.assistant.delete_conversation(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn event(name: &str, data: serde_json::Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

fn friendly(error: &Error) -> String {
    match error {
        Error::RateLimited { retry_after_secs } => format!(
            "You have asked a lot this hour. Try again in about {} minutes.",
            retry_after_secs.div_ceil(60)
        ),
        Error::External(reason) | Error::Unavailable(reason) => reason.clone(),
        Error::Forbidden => "The assistant is not available for this profile.".into(),
        _ => "The assistant could not answer.".into(),
    }
}

/// Ask the assistant. The answer streams as server-sent events: `conversation` (the
/// conversation, first), `delta` (`{"text"}` pieces as they are written), then `done`
/// (`{"message", "tools"}`, the stored answer) or `error` (`{"message"}`).
#[utoipa::path(
    post,
    path = "/api/v1/assistant/chat",
    request_body = AssistantChatRequest,
    responses(
        (status = 200, description = "Event stream with the answer", content_type = "text/event-stream", body = String),
        (status = 403, description = "Managed profiles cannot use the assistant"),
        (status = 404, description = "Conversation not found"),
        (status = 422, description = "Empty or too long message"),
        (status = 503, description = "The assistant is turned off")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn assistant_chat(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<AssistantChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let assistant = state.services.assistant.clone();
    let (conversation, turns) =
        assistant.begin_chat(&auth.actor, body.conversation_id, &body.text).await?;
    let backend = assistant
        .backend()
        .ok_or_else(|| Error::Unavailable("the assistant is turned off".into()))?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let _ = tx.send(event("conversation", json!(conversation)));
    let actor = auth.actor.clone();
    // The answer is stored even if the person closes the page meanwhile.
    tokio::spawn(async move {
        let deltas = tx.clone();
        let mut sink = move |piece: &str| {
            let _ = deltas.send(event("delta", json!({ "text": piece })));
        };
        let outcome = match backend.answer(&actor, turns, Some(&mut sink)).await {
            Ok(answer) => {
                let text = if answer.text.trim().is_empty() {
                    if answer.refused {
                        "I can't help with that one.".to_owned()
                    } else {
                        "I don't have an answer to that.".to_owned()
                    }
                } else {
                    answer.text
                };
                assistant
                    .finish_chat(&actor, conversation.id, &text, &answer.items)
                    .await
                    .map(|message| json!({ "message": message, "tools": answer.tools }))
            }
            Err(error) => Err(error),
        };
        let _ = match outcome {
            Ok(done) => tx.send(event("done", done)),
            Err(error) => {
                tracing::debug!(%error, "assistant chat failed");
                tx.send(event("error", json!({ "message": friendly(&error) })))
            }
        };
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|e| (Ok::<_, Infallible>(e), rx))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
