use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use mbl_core::UserId;
use mbl_db::repo::users::{HomeLayoutRequest, UpdateUserPreferences, UserPreferences};
use mbl_service::{CreateUser, UpdateUser};

use crate::dto::{CreateUserRequest, UpdateUserRequest, UserDto, UserProfileDto};
use crate::extract::{AdminUser, AuthUser};
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/users", get(list).post(create))
        .route("/api/v1/users/profiles", get(list_profiles))
        .route("/api/v1/users/me/preferences", get(get_preferences).patch(update_preferences))
        .route("/api/v1/users/me/home-layout", put(update_home_layout))
        .route("/api/v1/users/{id}", get(get_one).patch(update).delete(remove))
}

/// All users (admin).
#[utoipa::path(get, path = "/api/v1/users", tag = "users", operation_id = "list_users",
    responses((status = 200, body = Vec<UserDto>), (status = 403)))]
async fn list(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    let users = state.services.users.list(&auth.actor).await?;
    Ok(Json(users.into_iter().map(Into::into).collect()))
}

/// Create a user (admin).
#[utoipa::path(post, path = "/api/v1/users", tag = "users", operation_id = "create_user",
    request_body = CreateUserRequest,
    responses((status = 201, body = UserDto), (status = 403), (status = 409)))]
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

/// A user (admins, or the user themselves).
#[utoipa::path(get, path = "/api/v1/users/{id}", tag = "users", operation_id = "get_user",
    params(("id" = i64, Path)),
    responses((status = 200, body = UserDto), (status = 404)))]
async fn get_one(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<UserId>,
) -> Result<Json<UserDto>, ApiError> {
    Ok(Json(state.services.users.get(&auth.actor, id).await?.into()))
}

/// Update a user; only admins may change `isAdmin`.
#[utoipa::path(patch, path = "/api/v1/users/{id}", tag = "users", operation_id = "update_user",
    params(("id" = i64, Path)),
    request_body = UpdateUserRequest,
    responses((status = 200, body = UserDto), (status = 403), (status = 404)))]
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
    Ok(Json(state.services.users.update(&auth.actor, id, patch).await?.into()))
}

/// Delete a user (admin).
#[utoipa::path(delete, path = "/api/v1/users/{id}", tag = "users", operation_id = "delete_user",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 403), (status = 404)))]
async fn remove(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<UserId>,
) -> Result<StatusCode, ApiError> {
    state.services.users.delete(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Household user profiles for the profile switcher (authenticated).
#[utoipa::path(get, path = "/api/v1/users/profiles", tag = "users", operation_id = "list_user_profiles",
    responses((status = 200, body = Vec<UserProfileDto>), (status = 401)))]
async fn list_profiles(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<UserProfileDto>>, ApiError> {
    let profiles = state.services.users.list_profiles(&auth.actor).await?;
    Ok(Json(profiles.into_iter().map(Into::into).collect()))
}

/// Current user preferences (theme, accent, locale, subtitles, layout).
#[utoipa::path(get, path = "/api/v1/users/me/preferences", tag = "users", operation_id = "get_user_preferences",
    responses((status = 200, body = UserPreferences), (status = 401)))]
async fn get_preferences(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<UserPreferences>, ApiError> {
    Ok(Json(state.services.users.get_preferences(&auth.actor).await?))
}

/// Update current user preferences.
#[utoipa::path(patch, path = "/api/v1/users/me/preferences", tag = "users", operation_id = "update_user_preferences",
    request_body = UpdateUserPreferences,
    responses((status = 200, body = UserPreferences), (status = 400), (status = 401)))]
async fn update_preferences(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(patch): Json<UpdateUserPreferences>,
) -> Result<Json<UserPreferences>, ApiError> {
    Ok(Json(state.services.users.update_preferences(&auth.actor, patch).await?))
}

/// Reorder home hubs layout.
#[utoipa::path(put, path = "/api/v1/users/me/home-layout", tag = "users", operation_id = "update_home_layout",
    request_body = HomeLayoutRequest,
    responses((status = 200, body = UserPreferences), (status = 400), (status = 401)))]
async fn update_home_layout(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<HomeLayoutRequest>,
) -> Result<Json<UserPreferences>, ApiError> {
    Ok(Json(state.services.users.update_home_layout(&auth.actor, req).await?))
}
