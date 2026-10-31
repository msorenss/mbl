use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use mbl_core::Error;
use mbl_db::repo::settings::SettingsRepo;
use serde::{Deserialize, Serialize};

use crate::dto::{LoginResponse, SetupRequest, SetupStatus};
use crate::extract::{AdminUser, AuthUser, device_info};
use crate::routes::auth::login_response;
use crate::{ApiError, AppState, ServerInfo};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/system/health", get(system_health))
        .route("/api/v1/system/info", get(info))
        .route("/api/v1/system/setup", get(setup_status).post(setup))
        .route("/api/v1/system/settings", get(get_system_settings).patch(update_system_settings))
        .route("/api/v1/system/settings/test-tmdb", axum::routing::post(test_tmdb))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct Health {
    status: &'static str,
}

/// Liveness + database reachability.
#[utoipa::path(get, path = "/health", tag = "system", security(),
    responses((status = 200, body = Health), (status = 503)))]
async fn health(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    sqlx::query("SELECT 1").execute(&state.db.read).await.map_err(|e| {
        tracing::warn!(error = %e, "health check: database unavailable");
        mbl_core::Error::Unavailable("database".into())
    })?;
    Ok(Json(Health { status: "ok" }))
}

/// Detailed system health including SoC temperature, throttling/under-voltage flags, and platform profile defaults (M5.1, M5.2).
#[utoipa::path(get, path = "/api/v1/system/health", tag = "system", security(),
    responses((status = 200, body = mbl_service::SystemHealthReport)))]
async fn system_health(State(state): State<AppState>) -> Json<mbl_service::SystemHealthReport> {
    let admin_actor = mbl_core::Actor::admin(mbl_core::UserId(0));
    let active = state.services.playback.list_active(&admin_actor).len();
    Json(state.services.intelligence.system_health(active))
}

/// Server name, version and platform.
#[utoipa::path(get, path = "/api/v1/system/info", tag = "system", security(),
    responses((status = 200, body = ServerInfo)))]
async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json((*state.info).clone())
}

/// Whether the first admin still has to be created.
#[utoipa::path(get, path = "/api/v1/system/setup", tag = "system", security(),
    responses((status = 200, body = SetupStatus)))]
async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    Ok(Json(SetupStatus { needs_setup: state.services.auth.needs_setup().await? }))
}

/// Create the first admin account. Only allowed while no users exist.
#[utoipa::path(post, path = "/api/v1/system/setup", tag = "system", security(),
    request_body = SetupRequest,
    responses((status = 201, body = LoginResponse), (status = 409)))]
async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<Response, ApiError> {
    let device = device_info(&headers, req.device_name, req.client_name);
    let result = state
        .services
        .auth
        .setup(&req.username, &req.password, req.display_name.as_deref(), &device)
        .await?;
    Ok(login_response(StatusCode::CREATED, result, state.secure_cookies))
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettingsDto {
    pub tmdb_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmdb_api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSystemSettingsRequest {
    pub tmdb_api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestTmdbRequest {
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestTmdbResponse {
    pub success: bool,
    pub message: String,
}

/// Current server system settings, including TMDB metadata provider status.
#[utoipa::path(
    get,
    path = "/api/v1/system/settings",
    tag = "system",
    security(("bearer" = []), ("cookie" = [])),
    responses(
        (status = 200, body = SystemSettingsDto),
        (status = 401, description = "Unauthorized")
    )
)]
async fn get_system_settings(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<SystemSettingsDto>, ApiError> {
    let repo = SettingsRepo::new(state.db.clone());
    let saved_key = repo.get("tmdb_api_key").await?;
    let is_configured = saved_key.as_deref().is_some_and(|k| !k.trim().is_empty());
    let tmdb_api_key = if auth.actor.is_admin { saved_key } else { None };
    Ok(Json(SystemSettingsDto { tmdb_configured: is_configured, tmdb_api_key }))
}

/// Update system settings (admin only).
#[utoipa::path(
    patch,
    path = "/api/v1/system/settings",
    tag = "system",
    security(("bearer" = []), ("cookie" = [])),
    request_body = UpdateSystemSettingsRequest,
    responses(
        (status = 200, body = SystemSettingsDto),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (admin only)")
    )
)]
async fn update_system_settings(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(req): Json<UpdateSystemSettingsRequest>,
) -> Result<Json<SystemSettingsDto>, ApiError> {
    let repo = SettingsRepo::new(state.db.clone());
    if let Some(key) = req.tmdb_api_key {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            repo.delete("tmdb_api_key").await?;
        } else {
            repo.set("tmdb_api_key", trimmed).await?;
        }
    }
    let saved_key = repo.get("tmdb_api_key").await?;
    let is_configured = saved_key.as_deref().is_some_and(|k| !k.trim().is_empty());
    Ok(Json(SystemSettingsDto { tmdb_configured: is_configured, tmdb_api_key: saved_key }))
}

/// Test TMDB API key reachability and authentication (admin only).
#[utoipa::path(
    post,
    path = "/api/v1/system/settings/test-tmdb",
    tag = "system",
    security(("bearer" = []), ("cookie" = [])),
    request_body = TestTmdbRequest,
    responses(
        (status = 200, body = TestTmdbResponse),
        (status = 400, description = "Invalid API key or communication failure"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (admin only)")
    )
)]
async fn test_tmdb(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(req): Json<TestTmdbRequest>,
) -> Result<Json<TestTmdbResponse>, ApiError> {
    let key = if let Some(k) = req.api_key.filter(|k| !k.trim().is_empty()) {
        k.trim().to_string()
    } else {
        let repo = SettingsRepo::new(state.db.clone());
        repo.get("tmdb_api_key")
            .await?
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| Error::Validation("No TMDB API key provided or configured".into()))?
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(Error::internal)?;

    // v4 read access tokens are JWTs (>40 chars); v3 keys are 32 hex chars.
    let is_bearer = key.len() > 40;
    let mut req_builder = client.get("https://api.themoviedb.org/3/configuration");
    if is_bearer {
        req_builder = req_builder.bearer_auth(&key);
    } else {
        req_builder = req_builder.query(&[("api_key", &key)]);
    }

    let resp = req_builder
        .send()
        .await
        .map_err(|e| Error::External(format!("Failed to reach TMDB API: {e}")))?;

    if resp.status().is_success() {
        Ok(Json(TestTmdbResponse {
            success: true,
            message: "Successfully connected to TMDB API".into(),
        }))
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(status = %status, body = %body, "TMDB API verification failed");
        Err(Error::Validation(format!(
            "TMDB verification failed (status {}): please check that the API key is correct",
            status.as_u16()
        ))
        .into())
    }
}
