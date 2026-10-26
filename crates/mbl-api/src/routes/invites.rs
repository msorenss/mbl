//! Share invites endpoints (M6.2).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mbl_db::repo::m6::InviteRow;
use mbl_service::CreateInviteRequest;

use crate::extract::AdminUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/invites", post(create).get(list))
        .route("/api/v1/invites/{id}", delete(remove))
        .route("/api/v1/invites/inspect/{code}", get(inspect))
}

/// Create a new share invite (admin only).
#[utoipa::path(post, path = "/api/v1/invites", tag = "invites", operation_id = "create_invite",
    request_body = CreateInviteRequest,
    responses((status = 201, body = InviteRow), (status = 403)))]
pub(crate) async fn create(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Json(req): Json<CreateInviteRequest>,
) -> Result<(StatusCode, Json<InviteRow>), ApiError> {
    let invite = state.services.auth.create_invite(&auth.actor, req).await?;
    Ok((StatusCode::CREATED, Json(invite)))
}

/// List all share invites (admin only).
#[utoipa::path(get, path = "/api/v1/invites", tag = "invites", operation_id = "list_invites",
    responses((status = 200, body = Vec<InviteRow>), (status = 403)))]
pub(crate) async fn list(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
) -> Result<Json<Vec<InviteRow>>, ApiError> {
    Ok(Json(state.services.auth.list_invites(&auth.actor).await?))
}

/// Revoke/delete an invite (admin only).
#[utoipa::path(delete, path = "/api/v1/invites/{id}", tag = "invites", operation_id = "delete_invite",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 403), (status = 404)))]
pub(crate) async fn remove(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.services.auth.delete_invite(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Inspect an invite before registration (public).
#[utoipa::path(get, path = "/api/v1/invites/inspect/{code}", tag = "invites", operation_id = "inspect_invite",
    security(),
    params(("code" = String, Path)),
    responses((status = 200, body = InviteRow), (status = 404)))]
pub(crate) async fn inspect(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<InviteRow>, ApiError> {
    Ok(Json(state.services.auth.inspect_invite(&code).await?))
}
