//! HTTPS certificate management: local CA, Tailscale certificates, and listener settings.
//!
//! The server watches these settings and (re)starts its HTTPS listener without a restart.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::Error;
use mbl_db::repo::settings::SettingsRepo;
use mbl_remote::TailscaleClient;
use mbl_remote::tls::{clean_names, generate_local_ca, issue_server_cert, split_pair};
use serde::{Deserialize, Serialize};

use crate::extract::AdminUser;
use crate::{ApiError, AppState};

pub const DEFAULT_HTTPS_PORT: u16 = 8443;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/system/tls", get(get_tls).patch(update_tls))
        .route("/api/v1/system/tls/local-ca", post(issue_local_ca))
        .route("/api/v1/system/tls/tailscale", post(issue_tailscale))
        .route("/api/v1/system/tls/acme", post(issue_acme))
        .route("/api/v1/system/tls/ca.pem", get(download_ca))
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TlsStatus {
    /// Serve HTTPS when a certificate is available.
    pub enabled: bool,
    pub port: u16,
    /// `none`, `local_ca`, `tailscale`, or `letsencrypt`.
    pub mode: String,
    pub hostnames: Vec<String>,
    /// Certificate expiry in unix seconds.
    pub expires_at: Option<i64>,
    pub has_certificate: bool,
    /// The local root CA exists and can be downloaded for installation on devices.
    pub ca_available: bool,
    /// Configured Let's Encrypt contact email (if any).
    pub acme_email: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTlsRequest {
    pub enabled: Option<bool>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalCaRequest {
    /// DNS names and/or IP addresses the certificate must be valid for.
    pub hostnames: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcmeRequest {
    /// Public DNS domain name for the Let's Encrypt certificate.
    pub domain: String,
    /// Contact email address for Let's Encrypt expiration notices.
    pub email: Option<String>,
    /// Optional PEM certificate chain (if importing an existing Let's Encrypt / ACME certificate).
    pub cert_pem: Option<String>,
    /// Optional PEM private key (if importing an existing Let's Encrypt / ACME private key).
    pub key_pem: Option<String>,
}

async fn status(repo: &SettingsRepo) -> Result<TlsStatus, ApiError> {
    let enabled = repo.get("tls_enabled").await?.as_deref() == Some("true");
    let port = repo
        .get("tls_port")
        .await?
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_HTTPS_PORT);
    let hostnames = repo
        .get("tls_hostnames")
        .await?
        .map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    let has_certificate =
        repo.get("tls_cert_pem").await?.is_some() && repo.get("tls_key_pem").await?.is_some();
    Ok(TlsStatus {
        enabled,
        port,
        mode: repo.get("tls_mode").await?.unwrap_or_else(|| "none".into()),
        hostnames,
        expires_at: repo.get("tls_cert_expires_at").await?.and_then(|v| v.parse().ok()),
        has_certificate,
        ca_available: repo.get("tls_ca_cert_pem").await?.is_some(),
        acme_email: repo.get("tls_acme_email").await?,
    })
}

async fn store_cert(
    repo: &SettingsRepo,
    mode: &str,
    cert_pem: &str,
    key_pem: &str,
    expires_at: i64,
    hostnames: &[String],
) -> Result<(), ApiError> {
    repo.set("tls_cert_pem", cert_pem).await?;
    repo.set("tls_key_pem", key_pem).await?;
    repo.set("tls_cert_expires_at", &expires_at.to_string()).await?;
    repo.set("tls_hostnames", &hostnames.join(",")).await?;
    repo.set("tls_mode", mode).await?;
    Ok(())
}

/// Current HTTPS configuration and certificate status (admin only).
#[utoipa::path(get, path = "/api/v1/system/tls", tag = "system",
    security(("bearer" = []), ("cookie" = [])),
    responses((status = 200, body = TlsStatus), (status = 401), (status = 403)))]
async fn get_tls(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> Result<Json<TlsStatus>, ApiError> {
    Ok(Json(status(&SettingsRepo::new(state.db.clone())).await?))
}

/// Enable/disable the HTTPS listener or change its port (admin only).
#[utoipa::path(patch, path = "/api/v1/system/tls", tag = "system",
    security(("bearer" = []), ("cookie" = [])), request_body = UpdateTlsRequest,
    responses((status = 200, body = TlsStatus), (status = 400), (status = 401), (status = 403)))]
async fn update_tls(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(req): Json<UpdateTlsRequest>,
) -> Result<Json<TlsStatus>, ApiError> {
    let repo = SettingsRepo::new(state.db.clone());
    if let Some(port) = req.port {
        if port < 1024 {
            return Err(Error::Validation("HTTPS port must be 1024 or higher".into()).into());
        }
        repo.set("tls_port", &port.to_string()).await?;
    }
    if let Some(enabled) = req.enabled {
        if enabled && repo.get("tls_cert_pem").await?.is_none() {
            return Err(Error::Validation(
                "create or fetch a certificate before enabling HTTPS".into(),
            )
            .into());
        }
        repo.set("tls_enabled", if enabled { "true" } else { "false" }).await?;
    }
    Ok(Json(status(&repo).await?))
}

/// Create (or reuse) the local root CA and issue a server certificate (admin only).
#[utoipa::path(post, path = "/api/v1/system/tls/local-ca", tag = "system",
    security(("bearer" = []), ("cookie" = [])), request_body = LocalCaRequest,
    responses((status = 200, body = TlsStatus), (status = 400), (status = 401), (status = 403)))]
async fn issue_local_ca(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(req): Json<LocalCaRequest>,
) -> Result<Json<TlsStatus>, ApiError> {
    let repo = SettingsRepo::new(state.db.clone());
    let names = clean_names(&req.hostnames)?;
    let ca_key = match repo.get("tls_ca_key_pem").await? {
        Some(key) if repo.get("tls_ca_cert_pem").await?.is_some() => key,
        _ => {
            let ca =
                tokio::task::spawn_blocking(generate_local_ca).await.map_err(Error::internal)??;
            repo.set("tls_ca_cert_pem", &ca.cert_pem).await?;
            repo.set("tls_ca_key_pem", &ca.key_pem).await?;
            ca.key_pem
        }
    };
    let issue_names = names.clone();
    let pair = tokio::task::spawn_blocking(move || issue_server_cert(&ca_key, &issue_names))
        .await
        .map_err(Error::internal)??;
    store_cert(&repo, "local_ca", &pair.cert_pem, &pair.key_pem, pair.not_after, &names).await?;
    Ok(Json(status(&repo).await?))
}

/// Fetch a certificate for this node's MagicDNS name from the local Tailscale daemon (admin only).
#[utoipa::path(post, path = "/api/v1/system/tls/tailscale", tag = "system",
    security(("bearer" = []), ("cookie" = [])),
    responses((status = 200, body = TlsStatus), (status = 401), (status = 403), (status = 503)))]
async fn issue_tailscale(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> Result<Json<TlsStatus>, ApiError> {
    let repo = SettingsRepo::new(state.db.clone());
    let client = TailscaleClient::from_platform_default();
    let status_report = client.get_status().await?;
    let domain = status_report.magic_dns_name.filter(|d| !d.is_empty()).ok_or_else(|| {
        Error::Unavailable("Tailscale MagicDNS name is not available (is MagicDNS enabled?)".into())
    })?;
    let (cert_pem, key_pem) = split_pair(&client.fetch_cert_pair(&domain).await?)?;
    // Tailscale certificates are Let's Encrypt certificates valid for 90 days.
    let expires_at = now_secs() + 90 * 24 * 3600;
    store_cert(&repo, "tailscale", &cert_pem, &key_pem, expires_at, &[domain]).await?;
    Ok(Json(status(&repo).await?))
}

/// Configure or import a Let's Encrypt (ACME) certificate for a public domain (admin only).
#[utoipa::path(post, path = "/api/v1/system/tls/acme", tag = "system",
    security(("bearer" = []), ("cookie" = [])), request_body = AcmeRequest,
    responses((status = 200, body = TlsStatus), (status = 400), (status = 401), (status = 403), (status = 503)))]
async fn issue_acme(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(req): Json<AcmeRequest>,
) -> Result<Json<TlsStatus>, ApiError> {
    let names = clean_names(std::slice::from_ref(&req.domain))?;
    let domain = names[0].clone();
    if !domain.contains('.')
        || domain.ends_with(".local")
        || domain.parse::<std::net::IpAddr>().is_ok()
    {
        return Err(Error::Validation(
            "Let's Encrypt requires a public domain name (not an IP address or .local)".into(),
        )
        .into());
    }
    let repo = SettingsRepo::new(state.db.clone());
    if let Some(email) = req.email.as_deref().map(str::trim) {
        if email.is_empty() {
            repo.delete("tls_acme_email").await?;
        } else {
            repo.set("tls_acme_email", email).await?;
        }
    }

    let combined = match (req.cert_pem, req.key_pem) {
        (Some(c), Some(k)) if !c.trim().is_empty() && !k.trim().is_empty() => {
            format!("{c}\n{k}")
        }
        _ => {
            let live_dir = std::path::PathBuf::from("/etc/letsencrypt/live").join(&domain);
            let cert_path = live_dir.join("fullchain.pem");
            let key_path = live_dir.join("privkey.pem");
            match (
                tokio::fs::read_to_string(&cert_path).await,
                tokio::fs::read_to_string(&key_path).await,
            ) {
                (Ok(c), Ok(k)) => format!("{c}\n{k}"),
                _ => {
                    return Err(Error::Unavailable(format!(
                        "No Let's Encrypt certificate found for {domain} in /etc/letsencrypt/live/{domain} (obtain one via certbot/DNS-01 or paste the PEM certificate and private key)"
                    ))
                    .into());
                }
            }
        }
    };

    let (cert_pem, key_pem) = split_pair(&combined)
        .map_err(|_| Error::Validation("invalid PEM certificate chain or private key".into()))?;
    let expires_at = now_secs() + 90 * 24 * 3600;
    store_cert(&repo, "letsencrypt", &cert_pem, &key_pem, expires_at, &[domain]).await?;
    Ok(Json(status(&repo).await?))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Download the local root CA certificate to install on client devices (admin only).
#[utoipa::path(get, path = "/api/v1/system/tls/ca.pem", tag = "system",
    security(("bearer" = []), ("cookie" = [])),
    responses((status = 200, content_type = "application/x-x509-ca-cert", body = String),
        (status = 401), (status = 403), (status = 404)))]
async fn download_ca(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> Result<Response, ApiError> {
    let pem =
        SettingsRepo::new(state.db.clone()).get("tls_ca_cert_pem").await?.ok_or(Error::NotFound)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-x509-ca-cert")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"mbl-local-ca.crt\"")
        .body(axum::body::Body::from(pem))
        .map_err(|e| Error::internal(e).into())
}
