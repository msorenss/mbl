use axum::extract::{Path, Query, State};
use axum::http::header::{HeaderMap, SET_COOKIE};
use axum::http::{Extensions, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mbl_core::SessionId;
use mbl_db::repo::mcp::ApiTokenInfo;
use mbl_service::{CreateApiTokenRequest, CreatedApiToken, LoginResult};

use crate::dto::{LoginRequest, LoginResponse, SessionDto, UserDto};
use crate::extract::{AuthUser, SESSION_COOKIE, client_ip, device_info};
use crate::{ApiError, AppState};

const COOKIE_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/auth/sessions", get(list_sessions))
        .route("/api/v1/auth/sessions/{id}", delete(revoke_session))
        .route("/api/v1/auth/tokens", post(create_token).get(list_tokens))
        .route("/api/v1/auth/tokens/{id}", delete(revoke_token))
}

/// Log in; returns a bearer token and sets the session cookie.
#[utoipa::path(post, path = "/api/v1/auth/login", tag = "auth", security(),
    request_body = LoginRequest,
    responses((status = 200, body = LoginResponse), (status = 401), (status = 429)))]
async fn login(
    State(state): State<AppState>,
    extensions: Extensions,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    state.login_limiter.check(&client_ip(&extensions), &req.username)?;
    let device = device_info(&headers, req.device_name, req.client_name);
    let result = state.services.auth.login(&req.username, &req.password, &device).await;
    if result.is_err() {
        tracing::warn!(username = %req.username, ip = %client_ip(&extensions), "failed login");
    }
    Ok(login_response(StatusCode::OK, result?, state.secure_cookies))
}

/// Build the token response and set the session cookie.
pub(crate) fn login_response(status: StatusCode, result: LoginResult, secure: bool) -> Response {
    let cookie = session_cookie(&result.token, COOKIE_MAX_AGE_SECS, secure);
    let body = LoginResponse { token: result.token, user: result.user.into() };
    (status, [(SET_COOKIE, cookie)], Json(body)).into_response()
}

fn session_cookie(value: &str, max_age_secs: u64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure}"
    );
    // Tokens are base64url, so the cookie is always a valid header value.
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// End the current session.
#[utoipa::path(post, path = "/api/v1/auth/logout", tag = "auth",
    responses((status = 204)))]
async fn logout(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Response, ApiError> {
    state.services.auth.logout(auth.session_id).await?;
    let cookie = session_cookie("", 0, state.secure_cookies);
    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response())
}

/// The current user.
#[utoipa::path(get, path = "/api/v1/auth/me", tag = "auth",
    responses((status = 200, body = UserDto)))]
async fn me(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<UserDto>, ApiError> {
    Ok(Json(state.services.auth.me(&auth.actor).await?.into()))
}

/// The current user's sessions.
#[utoipa::path(get, path = "/api/v1/auth/sessions", tag = "auth",
    responses((status = 200, body = Vec<SessionDto>)))]
async fn list_sessions(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<SessionDto>>, ApiError> {
    let sessions = state.services.auth.list_sessions(&auth.actor).await?;
    Ok(Json(sessions.into_iter().map(|s| SessionDto::new(s, auth.session_id)).collect()))
}

/// Revoke one of the current user's sessions.
#[utoipa::path(delete, path = "/api/v1/auth/sessions/{id}", tag = "auth",
    params(("id" = i64, Path)), responses((status = 204), (status = 404)))]
async fn revoke_session(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<SessionId>,
) -> Result<StatusCode, ApiError> {
    state.services.auth.revoke_session(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Mint a new scoped API / MCP token. The plaintext token is returned only once.
#[utoipa::path(post, path = "/api/v1/auth/tokens", tag = "auth",
    request_body = CreateApiTokenRequest,
    responses(
        (status = 201, body = CreatedApiToken),
        (status = 403, description = "Requested scopes exceed user permissions"),
        (status = 422, description = "Invalid token request"),
    ))]
async fn create_token(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<CreateApiTokenRequest>,
) -> Result<(StatusCode, Json<CreatedApiToken>), ApiError> {
    let created = state.services.auth.create_api_token(&auth.actor, req).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[derive(Debug, Clone, Default, serde::Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListTokensParams {
    pub all: Option<bool>,
}

/// List active API tokens owned by the current user (or all tokens across users if admin and ?all=true).
#[utoipa::path(get, path = "/api/v1/auth/tokens", tag = "auth",
    params(("all" = Option<bool>, Query, description = "List tokens across all users (admin only)")),
    responses((status = 200, body = Vec<ApiTokenInfo>)))]
async fn list_tokens(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Query(params): Query<ListTokensParams>,
) -> Result<Json<Vec<ApiTokenInfo>>, ApiError> {
    let tokens = if params.all == Some(true) && auth.actor.is_admin {
        state.services.auth.list_all_api_tokens(&auth.actor).await?
    } else {
        state.services.auth.list_api_tokens(&auth.actor).await?
    };
    Ok(Json(tokens))
}

/// Revoke an API token.
#[utoipa::path(delete, path = "/api/v1/auth/tokens/{id}", tag = "auth",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404)))]
async fn revoke_token(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.services.auth.revoke_api_token(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
