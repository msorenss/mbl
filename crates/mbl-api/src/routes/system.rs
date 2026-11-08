use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::Error;
use mbl_db::repo::settings::SettingsRepo;
use serde::{Deserialize, Serialize};

use crate::dto::{LoginResponse, SetupRequest, SetupStatus};
use crate::extract::{AdminUser, AuthUser, device_info};
use crate::logs::ServerLogEntry;
use crate::routes::auth::login_response;
use crate::{ApiError, AppState, ServerInfo};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/system/health", get(system_health))
        .route("/api/v1/system/info", get(info))
        .route("/api/v1/system/logs", get(get_logs))
        .route("/api/v1/system/restart", post(restart))
        .route("/api/v1/system/endpoints", get(endpoints))
        .route("/api/v1/system/setup", get(setup_status).post(setup))
        .route("/api/v1/system/settings", get(get_system_settings).patch(update_system_settings))
        .route("/api/v1/system/settings/test-tmdb", axum::routing::post(test_tmdb))
        .route("/api/v1/system/tmdb/search", get(search_tmdb))
        .route("/api/v1/system/update", get(check_update))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestartAccepted {
    previous_started_at: i64,
}

/// Gracefully stop and rebuild the server runtime (admin only).
#[utoipa::path(post, path = "/api/v1/system/restart", tag = "system",
    responses((status = 202, body = RestartAccepted), (status = 401), (status = 403), (status = 503)))]
pub(crate) async fn restart(
    AdminUser(auth): AdminUser,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<RestartAccepted>), ApiError> {
    state.services.request_restart(&auth.actor)?;
    Ok((StatusCode::ACCEPTED, Json(RestartAccepted { previous_started_at: state.info.started_at })))
}

/// Recent server events for the administrator's log viewer.
#[utoipa::path(get, path = "/api/v1/system/logs", tag = "system",
    responses((status = 200, body = [ServerLogEntry]), (status = 401), (status = 403)))]
pub(crate) async fn get_logs(
    AdminUser(_): AdminUser,
    State(state): State<AppState>,
) -> ([(axum::http::HeaderName, &'static str); 1], Json<Vec<ServerLogEntry>>) {
    ([(header::CACHE_CONTROL, "no-store")], Json(state.recent_logs.snapshot()))
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

/// Addresses a client may use to reach this server, plus the local CA fingerprint for pinning.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerEndpoints {
    pub server_id: String,
    pub version: String,
    /// Configured externally reachable URL, if any.
    pub public_url: Option<String>,
    /// `http://<lan-ip>:<port>` for every non-loopback interface address.
    pub lan_urls: Vec<String>,
    /// MagicDNS and tailnet IP URLs when Tailscale is running.
    pub tailnet_urls: Vec<String>,
    /// HTTPS listener port when TLS is enabled.
    pub https_port: Option<u16>,
    /// Lower-case hex SHA-256 of the MBL Local CA certificate (DER), if one exists.
    pub ca_sha256: Option<String>,
}

/// Client discovery: LAN, public and tailnet URLs plus the local CA fingerprint. Contains no secrets.
#[utoipa::path(get, path = "/api/v1/system/endpoints", tag = "system", security(),
    responses((status = 200, body = ServerEndpoints)))]
pub(crate) async fn endpoints(
    State(state): State<AppState>,
) -> Result<Json<ServerEndpoints>, ApiError> {
    let port = state.info.listen.port;
    let repo = SettingsRepo::new(state.db.clone());
    let https_port = if repo.get("tls_enabled").await?.as_deref() == Some("true") {
        Some(
            repo.get("tls_port")
                .await?
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(crate::routes::tls::DEFAULT_HTTPS_PORT),
        )
    } else {
        None
    };
    let ca_sha256 = repo.get("tls_ca_cert_pem").await?.as_deref().and_then(pem_sha256_hex);

    let lan_urls = tokio::task::spawn_blocking(move || lan_urls(port)).await.unwrap_or_default();

    let tailscale = state.services.remote.tailscale().clone();
    let tailnet_urls =
        match tokio::time::timeout(std::time::Duration::from_secs(2), tailscale.get_status()).await
        {
            Ok(Ok(status)) if status.backend_state == "Running" => {
                let mut urls = Vec::new();
                if let Some(name) = status.magic_dns_name.filter(|n| !n.is_empty()) {
                    urls.push(format!("http://{}:{port}", name.trim_end_matches('.')));
                }
                urls.extend(status.tailnet_ips.iter().map(|ip| url_for_ip(ip, port)));
                urls
            }
            _ => Vec::new(),
        };

    let public_url = Some(state.info.listen.public_url.trim().trim_end_matches('/'))
        .filter(|u| !u.is_empty())
        .map(str::to_string);
    Ok(Json(ServerEndpoints {
        server_id: state.info.server_id.clone(),
        version: state.info.version.to_string(),
        public_url,
        lan_urls,
        tailnet_urls,
        https_port,
        ca_sha256,
    }))
}

fn url_for_ip(ip: &str, port: u16) -> String {
    if ip.contains(':') { format!("http://[{ip}]:{port}") } else { format!("http://{ip}:{port}") }
}

fn lan_urls(port: u16) -> Vec<String> {
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut v4 = Vec::new();
    for iface in ifaces {
        let ip = iface.ip();
        // Tailnet (100.64/10) addresses are reported separately; skip loopback and link-local.
        let skip = match ip {
            std::net::IpAddr::V4(a) => {
                a.is_loopback()
                    || a.is_link_local()
                    || (a.octets()[0] == 100 && (a.octets()[1] & 0xc0) == 64)
            }
            std::net::IpAddr::V6(_) => true,
        };
        if !skip {
            v4.push(url_for_ip(&ip.to_string(), port));
        }
    }
    v4.sort();
    v4.dedup();
    v4
}

/// SHA-256 over the DER body of the first certificate in `pem`, as lower-case hex.
fn pem_sha256_hex(pem: &str) -> Option<String> {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    let body: String = pem
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN CERTIFICATE-----"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END"))
        .map(str::trim)
        .collect();
    let der = base64::engine::general_purpose::STANDARD.decode(body).ok()?;
    if der.is_empty() {
        return None;
    }
    Some(Sha256::digest(&der).iter().map(|b| format!("{b:02x}")).collect())
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
    pub server_name: String,
    pub tmdb_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmdb_api_key: Option<String>,
    pub mqtt_enabled: bool,
    pub mqtt_broker_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mqtt_username: Option<String>,
    pub mqtt_topic_prefix: String,
    pub dlna_enabled: bool,
    pub dlna_friendly_name: String,
    pub auto_scan_on_change: bool,
    pub periodic_scan_interval_minutes: u32,
    pub auto_fetch_missing_metadata: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hailo_vision_hef_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hailo_vlm_hef_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hailo_llm_hef_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hailo_whisper_hef_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSystemSettingsRequest {
    pub server_name: Option<String>,
    pub tmdb_api_key: Option<String>,
    pub mqtt_enabled: Option<bool>,
    pub mqtt_broker_url: Option<String>,
    pub mqtt_username: Option<String>,
    pub mqtt_password: Option<String>,
    pub mqtt_topic_prefix: Option<String>,
    pub dlna_enabled: Option<bool>,
    pub dlna_friendly_name: Option<String>,
    pub auto_scan_on_change: Option<bool>,
    pub periodic_scan_interval_minutes: Option<u32>,
    pub auto_fetch_missing_metadata: Option<bool>,
    pub hailo_vision_hef_path: Option<String>,
    pub hailo_vlm_hef_path: Option<String>,
    pub hailo_llm_hef_path: Option<String>,
    pub hailo_whisper_hef_path: Option<String>,
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

#[derive(Debug, Clone, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct SearchTmdbQuery {
    pub query: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TmdbSearchHit {
    pub id: i64,
    pub title: String,
    pub kind: String,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TmdbSearchResponse {
    pub configured: bool,
    pub results: Vec<TmdbSearchHit>,
}

/// Current server system settings, including TMDB metadata, MQTT, and DLNA configuration.
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
    let server_name = repo.get("server_name").await?.unwrap_or_else(|| "Media Box Lounge".into());
    let saved_key = repo.get("tmdb_api_key").await?;
    let is_configured = saved_key.as_deref().is_some_and(|k| !k.trim().is_empty());
    let tmdb_api_key = if auth.actor.is_admin { saved_key } else { None };

    let mqtt_enabled = repo.get("mqtt_enabled").await?.as_deref() == Some("true");
    let mqtt_broker_url =
        repo.get("mqtt_broker_url").await?.unwrap_or_else(|| "mqtt://localhost:1883".into());
    let mqtt_username = if auth.actor.is_admin { repo.get("mqtt_username").await? } else { None };
    let mqtt_topic_prefix =
        repo.get("mqtt_topic_prefix").await?.unwrap_or_else(|| "mbl/media_lounge_box".into());

    let dlna_enabled = repo.get("dlna_enabled").await?.map(|v| v != "false").unwrap_or(true);
    let dlna_friendly_name =
        repo.get("dlna_friendly_name").await?.unwrap_or_else(|| "Media Box Lounge".into());

    let auto_scan_on_change =
        repo.get("auto_scan_on_change").await?.map(|v| v != "false").unwrap_or(true);
    let periodic_scan_interval_minutes = repo
        .get("periodic_scan_interval_minutes")
        .await?
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(60);
    let auto_fetch_missing_metadata =
        repo.get("auto_fetch_missing_metadata").await?.map(|v| v != "false").unwrap_or(true);
    let hailo_vision_hef_path = repo.get("hailo_vision_hef_path").await?;
    let hailo_vlm_hef_path = repo.get("hailo_vlm_hef_path").await?;
    let hailo_llm_hef_path = repo.get("hailo_llm_hef_path").await?;
    let hailo_whisper_hef_path = repo.get("hailo_whisper_hef_path").await?;

    mbl_hwaccel::hailo::set_hailo_model_overrides(mbl_hwaccel::hailo::HailoModelOverrides {
        vision_hef_path: hailo_vision_hef_path.clone(),
        vlm_hef_path: hailo_vlm_hef_path.clone(),
        llm_hef_path: hailo_llm_hef_path.clone(),
        whisper_hef_path: hailo_whisper_hef_path.clone(),
    });

    Ok(Json(SystemSettingsDto {
        server_name,
        tmdb_configured: is_configured,
        tmdb_api_key,
        mqtt_enabled,
        mqtt_broker_url,
        mqtt_username,
        mqtt_topic_prefix,
        dlna_enabled,
        dlna_friendly_name,
        auto_scan_on_change,
        periodic_scan_interval_minutes,
        auto_fetch_missing_metadata,
        hailo_vision_hef_path,
        hailo_vlm_hef_path,
        hailo_llm_hef_path,
        hailo_whisper_hef_path,
    }))
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
    if let Some(name) = req.server_name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            repo.delete("server_name").await?;
        } else {
            repo.set("server_name", trimmed).await?;
        }
    }
    if let Some(key) = req.tmdb_api_key {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            repo.delete("tmdb_api_key").await?;
        } else {
            repo.set("tmdb_api_key", trimmed).await?;
        }
    }
    if let Some(enabled) = req.mqtt_enabled {
        repo.set("mqtt_enabled", if enabled { "true" } else { "false" }).await?;
    }
    if let Some(url) = req.mqtt_broker_url {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            repo.delete("mqtt_broker_url").await?;
        } else {
            repo.set("mqtt_broker_url", trimmed).await?;
        }
    }
    if let Some(user) = req.mqtt_username {
        let trimmed = user.trim();
        if trimmed.is_empty() {
            repo.delete("mqtt_username").await?;
        } else {
            repo.set("mqtt_username", trimmed).await?;
        }
    }
    if let Some(pass) = req.mqtt_password {
        let trimmed = pass.trim();
        if trimmed.is_empty() {
            repo.delete("mqtt_password").await?;
        } else {
            repo.set("mqtt_password", trimmed).await?;
        }
    }
    if let Some(prefix) = req.mqtt_topic_prefix {
        let trimmed = prefix.trim();
        if trimmed.is_empty() {
            repo.delete("mqtt_topic_prefix").await?;
        } else {
            repo.set("mqtt_topic_prefix", trimmed).await?;
        }
    }
    if let Some(enabled) = req.dlna_enabled {
        repo.set("dlna_enabled", if enabled { "true" } else { "false" }).await?;
    }
    if let Some(name) = req.dlna_friendly_name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            repo.delete("dlna_friendly_name").await?;
        } else {
            repo.set("dlna_friendly_name", trimmed).await?;
        }
    }
    if let Some(enabled) = req.auto_scan_on_change {
        repo.set("auto_scan_on_change", if enabled { "true" } else { "false" }).await?;
    }
    if let Some(mins) = req.periodic_scan_interval_minutes {
        repo.set("periodic_scan_interval_minutes", &mins.to_string()).await?;
    }
    if let Some(enabled) = req.auto_fetch_missing_metadata {
        repo.set("auto_fetch_missing_metadata", if enabled { "true" } else { "false" }).await?;
    }
    for (key, val_opt) in [
        ("hailo_vision_hef_path", req.hailo_vision_hef_path),
        ("hailo_vlm_hef_path", req.hailo_vlm_hef_path),
        ("hailo_llm_hef_path", req.hailo_llm_hef_path),
        ("hailo_whisper_hef_path", req.hailo_whisper_hef_path),
    ] {
        if let Some(val) = val_opt {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                repo.delete(key).await?;
            } else {
                let p = std::path::Path::new(trimmed);
                if !p.is_file() || p.extension().and_then(|e| e.to_str()) != Some("hef") {
                    return Err(Error::Validation(format!(
                        "HEF model path must point to an existing .hef file: {trimmed}"
                    ))
                    .into());
                }
                repo.set(key, trimmed).await?;
            }
        }
    }

    let server_name = repo.get("server_name").await?.unwrap_or_else(|| "Media Box Lounge".into());
    let saved_key = repo.get("tmdb_api_key").await?;
    let is_configured = saved_key.as_deref().is_some_and(|k| !k.trim().is_empty());
    let mqtt_enabled = repo.get("mqtt_enabled").await?.as_deref() == Some("true");
    let mqtt_broker_url =
        repo.get("mqtt_broker_url").await?.unwrap_or_else(|| "mqtt://localhost:1883".into());
    let mqtt_username = repo.get("mqtt_username").await?;
    let mqtt_topic_prefix =
        repo.get("mqtt_topic_prefix").await?.unwrap_or_else(|| "mbl/media_lounge_box".into());
    let dlna_enabled = repo.get("dlna_enabled").await?.map(|v| v != "false").unwrap_or(true);
    let dlna_friendly_name =
        repo.get("dlna_friendly_name").await?.unwrap_or_else(|| "Media Box Lounge".into());
    let auto_scan_on_change =
        repo.get("auto_scan_on_change").await?.map(|v| v != "false").unwrap_or(true);
    let periodic_scan_interval_minutes = repo
        .get("periodic_scan_interval_minutes")
        .await?
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(60);
    let auto_fetch_missing_metadata =
        repo.get("auto_fetch_missing_metadata").await?.map(|v| v != "false").unwrap_or(true);
    let hailo_vision_hef_path = repo.get("hailo_vision_hef_path").await?;
    let hailo_vlm_hef_path = repo.get("hailo_vlm_hef_path").await?;
    let hailo_llm_hef_path = repo.get("hailo_llm_hef_path").await?;
    let hailo_whisper_hef_path = repo.get("hailo_whisper_hef_path").await?;

    mbl_hwaccel::hailo::set_hailo_model_overrides(mbl_hwaccel::hailo::HailoModelOverrides {
        vision_hef_path: hailo_vision_hef_path.clone(),
        vlm_hef_path: hailo_vlm_hef_path.clone(),
        llm_hef_path: hailo_llm_hef_path.clone(),
        whisper_hef_path: hailo_whisper_hef_path.clone(),
    });

    Ok(Json(SystemSettingsDto {
        server_name,
        tmdb_configured: is_configured,
        tmdb_api_key: saved_key,
        mqtt_enabled,
        mqtt_broker_url,
        mqtt_username,
        mqtt_topic_prefix,
        dlna_enabled,
        dlna_friendly_name,
        auto_scan_on_change,
        periodic_scan_interval_minutes,
        auto_fetch_missing_metadata,
        hailo_vision_hef_path,
        hailo_vlm_hef_path,
        hailo_llm_hef_path,
        hailo_whisper_hef_path,
    }))
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

/// Search TMDB for movies or TV shows (authenticated users).
#[utoipa::path(
    get,
    path = "/api/v1/system/tmdb/search",
    tag = "system",
    security(("bearer" = []), ("cookie" = [])),
    params(SearchTmdbQuery),
    responses(
        (status = 200, body = TmdbSearchResponse),
        (status = 401, description = "Unauthorized")
    )
)]
async fn search_tmdb(
    State(state): State<AppState>,
    AuthUser(_auth): AuthUser,
    Query(params): Query<SearchTmdbQuery>,
) -> Result<Json<TmdbSearchResponse>, ApiError> {
    let repo = SettingsRepo::new(state.db.clone());
    let key = repo.get("tmdb_api_key").await?;
    let Some(key) = key.filter(|k| !k.trim().is_empty()) else {
        return Ok(Json(TmdbSearchResponse { configured: false, results: vec![] }));
    };

    let trimmed_query = params.query.trim();
    if trimmed_query.is_empty() {
        return Ok(Json(TmdbSearchResponse { configured: true, results: vec![] }));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(Error::internal)?;

    let is_bearer = key.len() > 40;
    let endpoint = match params.kind.as_deref() {
        Some("movie") => "https://api.themoviedb.org/3/search/movie",
        Some("tv") | Some("show") => "https://api.themoviedb.org/3/search/tv",
        _ => "https://api.themoviedb.org/3/search/multi",
    };

    let mut req = client.get(endpoint).query(&[("query", trimmed_query)]);
    if is_bearer {
        req = req.bearer_auth(&key);
    } else {
        req = req.query(&[("api_key", &key)]);
    }

    let resp =
        req.send().await.map_err(|e| Error::External(format!("Failed to reach TMDB: {e}")))?;
    if !resp.status().is_success() {
        return Ok(Json(TmdbSearchResponse { configured: true, results: vec![] }));
    }

    #[derive(Deserialize)]
    struct TmdbRawItem {
        id: i64,
        title: Option<String>,
        name: Option<String>,
        media_type: Option<String>,
        release_date: Option<String>,
        first_air_date: Option<String>,
        overview: Option<String>,
        poster_path: Option<String>,
    }

    #[derive(Deserialize)]
    struct TmdbRawResponse {
        #[serde(default)]
        results: Vec<TmdbRawItem>,
    }

    let raw: TmdbRawResponse = resp.json().await.unwrap_or(TmdbRawResponse { results: vec![] });
    let hits = raw
        .results
        .into_iter()
        .filter_map(|item| {
            let title = item.title.or(item.name)?;
            let kind = item.media_type.unwrap_or_else(|| {
                if params.kind.as_deref() == Some("tv") || params.kind.as_deref() == Some("show") {
                    "show".to_string()
                } else {
                    "movie".to_string()
                }
            });
            let normalized_kind = if kind == "tv" { "show".to_string() } else { kind };
            if normalized_kind != "movie" && normalized_kind != "show" {
                return None;
            }
            let date = item.release_date.or(item.first_air_date);
            let year = date.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse::<i32>().ok());
            let poster_url =
                item.poster_path.map(|p| format!("https://image.tmdb.org/t/p/w300{p}"));
            Some(TmdbSearchHit {
                id: item.id,
                title,
                kind: normalized_kind,
                year,
                overview: item.overview.filter(|o| !o.is_empty()),
                poster_url,
            })
        })
        .take(10)
        .collect();

    Ok(Json(TmdbSearchResponse { configured: true, results: hits }))
}

/// GitHub Release API DTOs
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    html_url: String,
    assets: Option<Vec<GithubAsset>>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Query parameters for software update checking.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UpdateCheckQuery {
    /// When true, bypass the 15-minute in-memory cache and query GitHub directly.
    pub force: Option<bool>,
}

/// Information about the latest available MBL server release on GitHub.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResponse {
    /// Currently running server version.
    pub current_version: String,
    /// Latest release version tag on GitHub.
    pub latest_version: String,
    /// True if GitHub has a newer version than currently running.
    pub update_available: bool,
    /// Release title from GitHub.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_name: Option<String>,
    /// Release notes / changelog in markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    /// Timestamp when this release was published on GitHub.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// Link to the release page on GitHub.
    pub html_url: String,
    /// Direct download URL for the recommended binary/installer for this platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// Filename of the recommended asset (e.g. .msi on Windows, .deb on Debian/Pi).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_name: Option<String>,
    /// ISO 8601 timestamp of when the check was performed.
    pub checked_at: String,
    /// Error message if reaching GitHub or checking updates failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

static UPDATE_CACHE: tokio::sync::Mutex<Option<(std::time::Instant, UpdateCheckResponse)>> =
    tokio::sync::Mutex::const_new(None);

fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches('v');
    let mut parts = s.split('.');
    let maj: u32 = parts.next()?.parse().ok()?;
    let min: u32 = parts.next()?.parse().ok()?;
    let patch_str = parts.next().unwrap_or("0");
    let patch_num = patch_str.split('-').next().unwrap_or(patch_str);
    let patch: u32 = patch_num.parse().ok()?;
    Some((maj, min, patch))
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => latest.trim_start_matches('v') != current.trim_start_matches('v'),
    }
}

fn find_recommended_asset<'a>(
    assets: &'a [GithubAsset],
    os: &str,
    arch: &str,
) -> Option<(&'a str, &'a str)> {
    if os == "windows" {
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".msi")) {
            return Some((&a.name, &a.browser_download_url));
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with("-windows-x86_64.zip")) {
            return Some((&a.name, &a.browser_download_url));
        }
    } else if os == "linux" {
        if arch == "aarch64" || arch == "arm64" {
            if let Some(a) =
                assets.iter().find(|a| a.name.starts_with("mbl_") && a.name.ends_with("_arm64.deb"))
            {
                return Some((&a.name, &a.browser_download_url));
            }
            if let Some(a) = assets
                .iter()
                .find(|a| a.name.starts_with("mbl-v") && a.name.ends_with("-linux-arm64.tar.gz"))
            {
                return Some((&a.name, &a.browser_download_url));
            }
        } else {
            if let Some(a) =
                assets.iter().find(|a| a.name.starts_with("mbl_") && a.name.ends_with("_amd64.deb"))
            {
                return Some((&a.name, &a.browser_download_url));
            }
            if let Some(a) = assets
                .iter()
                .find(|a| a.name.starts_with("mbl-v") && a.name.ends_with("-linux-x86_64.tar.gz"))
            {
                return Some((&a.name, &a.browser_download_url));
            }
        }
    }
    None
}

/// Check for available MBL server updates from GitHub Releases.
#[utoipa::path(get, path = "/api/v1/system/update", tag = "system",
    params(UpdateCheckQuery),
    responses((status = 200, body = UpdateCheckResponse)))]
pub async fn check_update(
    State(state): State<AppState>,
    Query(query): Query<UpdateCheckQuery>,
) -> Result<Json<UpdateCheckResponse>, ApiError> {
    let current_version = state.info.version.to_string();
    let force = query.force.unwrap_or(false);

    if !force {
        let cache = UPDATE_CACHE.lock().await;
        if let Some((instant, ref res)) = *cache
            && instant.elapsed() < std::time::Duration::from_secs(15 * 60)
        {
            return Ok(Json(res.clone()));
        }
    }

    let checked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
    let url = "https://api.github.com/repos/msorenss/mbl/releases/latest";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| Error::Internal(format!("Failed to build HTTP client: {e}").into()))?;

    let user_agent = format!("mbl-server/{current_version}");
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
        .send()
        .await;

    let res = match response {
        Ok(resp) if resp.status().is_success() => match resp.json::<GithubRelease>().await {
            Ok(rel) => {
                let latest_raw = rel.tag_name.trim_start_matches('v').to_string();
                let update_available = is_newer_version(&latest_raw, &current_version);
                let (asset_name, download_url) = rel
                    .assets
                    .as_deref()
                    .and_then(|assets| {
                        find_recommended_asset(assets, state.info.os, state.info.arch)
                    })
                    .map(|(n, u)| (Some(n.to_string()), Some(u.to_string())))
                    .unwrap_or((None, None));

                UpdateCheckResponse {
                    current_version,
                    latest_version: latest_raw,
                    update_available,
                    release_name: rel.name,
                    release_notes: rel.body,
                    published_at: rel.published_at,
                    html_url: rel.html_url,
                    download_url,
                    asset_name,
                    checked_at,
                    error: None,
                }
            }
            Err(err) => UpdateCheckResponse {
                latest_version: current_version.clone(),
                current_version,
                update_available: false,
                release_name: None,
                release_notes: None,
                published_at: None,
                html_url: "https://github.com/msorenss/mbl/releases".into(),
                download_url: None,
                asset_name: None,
                checked_at,
                error: Some(format!("Failed to parse GitHub response: {err}")),
            },
        },
        Ok(resp) => UpdateCheckResponse {
            latest_version: current_version.clone(),
            current_version,
            update_available: false,
            release_name: None,
            release_notes: None,
            published_at: None,
            html_url: "https://github.com/msorenss/mbl/releases".into(),
            download_url: None,
            asset_name: None,
            checked_at,
            error: Some(format!("GitHub API returned HTTP {}", resp.status())),
        },
        Err(err) => UpdateCheckResponse {
            latest_version: current_version.clone(),
            current_version,
            update_available: false,
            release_name: None,
            release_notes: None,
            published_at: None,
            html_url: "https://github.com/msorenss/mbl/releases".into(),
            download_url: None,
            asset_name: None,
            checked_at,
            error: Some(format!("Could not reach GitHub: {err}")),
        },
    };

    let mut cache = UPDATE_CACHE.lock().await;
    *cache = Some((std::time::Instant::now(), res.clone()));

    Ok(Json(res))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ca_fingerprint_hashes_the_first_certificate_der() {
        // "YWJj" is base64 for the bytes "abc".
        let pem = "junk\n-----BEGIN CERTIFICATE-----\nYW\nJj\n-----END CERTIFICATE-----\n\
                   -----BEGIN CERTIFICATE-----\nZGVm\n-----END CERTIFICATE-----\n";
        assert_eq!(
            pem_sha256_hex(pem).as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(pem_sha256_hex("no certificate"), None);
        assert_eq!(
            pem_sha256_hex("-----BEGIN CERTIFICATE-----\n!!\n-----END CERTIFICATE-----"),
            None
        );
    }

    #[test]
    fn semver_comparison() {
        assert_eq!(parse_semver("v1.3.1"), Some((1, 3, 1)));
        assert_eq!(parse_semver("1.3.0"), Some((1, 3, 0)));
        assert_eq!(parse_semver("v2.0.0-beta"), Some((2, 0, 0)));

        assert!(is_newer_version("1.3.1", "1.3.0"));
        assert!(is_newer_version("v1.3.1", "1.3.0"));
        assert!(is_newer_version("2.0.0", "1.3.1"));
        assert!(!is_newer_version("1.3.0", "1.3.0"));
        assert!(!is_newer_version("1.2.9", "1.3.0"));
    }

    #[test]
    fn recommended_asset_selection() {
        let assets = vec![
            GithubAsset {
                name: "mbl-1.3.1-windows-x86_64.msi".into(),
                browser_download_url: "https://example.com/mbl.msi".into(),
            },
            GithubAsset {
                name: "mbl-v1.3.1-windows-x86_64.zip".into(),
                browser_download_url: "https://example.com/mbl.zip".into(),
            },
            GithubAsset {
                name: "mbl_1.3.1-1_arm64.deb".into(),
                browser_download_url: "https://example.com/mbl_arm64.deb".into(),
            },
            GithubAsset {
                name: "mbl_1.3.1-1_amd64.deb".into(),
                browser_download_url: "https://example.com/mbl_amd64.deb".into(),
            },
        ];

        // Windows prefers MSI
        let win = find_recommended_asset(&assets, "windows", "x86_64");
        assert_eq!(win, Some(("mbl-1.3.1-windows-x86_64.msi", "https://example.com/mbl.msi")));

        // Linux ARM64 (Raspberry Pi) prefers arm64.deb
        let arm = find_recommended_asset(&assets, "linux", "aarch64");
        assert_eq!(arm, Some(("mbl_1.3.1-1_arm64.deb", "https://example.com/mbl_arm64.deb")));

        // Linux x86_64 prefers amd64.deb
        let x86 = find_recommended_asset(&assets, "linux", "x86_64");
        assert_eq!(x86, Some(("mbl_1.3.1-1_amd64.deb", "https://example.com/mbl_amd64.deb")));
    }
}
