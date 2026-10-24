use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use mbl_core::UserId;
use mbl_service::{CreateUser, UpdateUser};

use crate::dto::{CreateUserRequest, UpdateUserRequest, UserDto};
use crate::extract::{AdminUser, AuthUser};
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/users", get(list).post(create))
        .route(
            "/api/v1/users/{id}",
            get(get_one).patch(update).delete(remove),
        )
}

async fn list(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    let users = state.services.users.list(&auth.actor).await?;
    Ok(Json(users.into_iter().map(Into::into).collect()))
}

async fn create(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserDto>), ApiError> {
    let input = CreateUser {
        username: req.username,
        password: req.password,
        display_name: req.display_name,
        is_admin: req.is_admin,
    };
    let user = state.services.users.create(&auth.actor, input).await?;
    Ok((StatusCode::CREATED, Json(user.into())))
}

async fn get_one(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<UserId>,
) -> Result<Json<UserDto>, ApiError> {
    Ok(Json(
        state.services.users.get(&auth.actor, id).await?.into(),
    ))
}

async fn update(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<UserId>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UserDto>, ApiError> {
    let patch = UpdateUser {
        display_name: req.display_name,
        password: req.password,
        is_admin: req.is_admin,
    };
    Ok(Json(
        state
            .services
            .users
            .update(&auth.actor, id, patch)
            .await?
            .into(),
    ))
}

async fn remove(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<UserId>,
) -> Result<StatusCode, ApiError> {
    state.services.users.delete(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
