use axum::extract::{Path, State};
use axum::http::header::{HeaderMap, SET_COOKIE};
use axum::http::{Extensions, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mbl_core::SessionId;
use mbl_service::LoginResult;

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
}

async fn login(
    State(state): State<AppState>,
    extensions: Extensions,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    state
        .login_limiter
        .check(&client_ip(&extensions), &req.username)?;
    let device = device_info(&headers, req.device_name, req.client_name);
    let result = state
        .services
        .auth
        .login(&req.username, &req.password, &device)
        .await;
    if result.is_err() {
        tracing::warn!(username = %req.username, ip = %client_ip(&extensions), "failed login");
    }
    Ok(login_response(
        StatusCode::OK,
        result?,
        state.secure_cookies,
    ))
}

/// Build the token response and set the session cookie.
pub(crate) fn login_response(status: StatusCode, result: LoginResult, secure: bool) -> Response {
    let cookie = session_cookie(&result.token, COOKIE_MAX_AGE_SECS, secure);
    let body = LoginResponse {
        token: result.token,
        user: result.user.into(),
    };
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

async fn logout(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Response, ApiError> {
    state.services.auth.logout(auth.session_id).await?;
    let cookie = session_cookie("", 0, state.secure_cookies);
    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response())
}

async fn me(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<UserDto>, ApiError> {
    Ok(Json(state.services.auth.me(&auth.actor).await?.into()))
}

async fn list_sessions(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<SessionDto>>, ApiError> {
    let sessions = state.services.auth.list_sessions(&auth.actor).await?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|s| SessionDto::new(s, auth.session_id))
            .collect(),
    ))
}

async fn revoke_session(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<SessionId>,
) -> Result<StatusCode, ApiError> {
    state.services.auth.revoke_session(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
