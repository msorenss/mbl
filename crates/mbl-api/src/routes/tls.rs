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
use mbl_service::certificates::{self, Request as CertificateRequest};
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
    pub job_state: String,
    pub job_error: Option<String>,
    pub automatic_renewal: bool,
    pub challenge_port: u16,
    /// Why the HTTPS listener is not serving the configured certificate and port, if it is not.
    pub listener_error: Option<String>,
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
    /// Required for automatic issuance to accept the certificate authority terms.
    #[serde(default)]
    pub accept_terms: bool,
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
        job_state: repo.get("tls_job_state").await?.unwrap_or_else(|| "idle".into()),
        job_error: repo.get("tls_job_error").await?.filter(|s| !s.is_empty()),
        automatic_renewal: repo.get("tls_renewal").await?.is_some_and(|v| !v.is_empty()),
        challenge_port: certificates::CHALLENGE_PORT,
        listener_error: repo.get("tls_listener_error").await?.filter(|s| !s.is_empty()),
    })
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
    // Validate the whole request before storing any part of it.
    let mut changes: Vec<(&str, String)> = Vec::new();
    if let Some(port) = req.port {
        if port < 1024 {
            return Err(Error::Validation("HTTPS port must be 1024 or higher".into()).into());
        }
        if port == certificates::CHALLENGE_PORT {
            return Err(Error::Validation(format!(
                "Port {port} is reserved for certificate verification; choose another HTTPS port"
            ))
            .into());
        }
        changes.push(("tls_port", port.to_string()));
    }
    if let Some(enabled) = req.enabled {
        let has_pair = repo.get("tls_cert_pem").await?.is_some_and(|v| !v.is_empty())
            && repo.get("tls_key_pem").await?.is_some_and(|v| !v.is_empty());
        if enabled && !has_pair {
            return Err(Error::Validation(
                "create or fetch a certificate before enabling HTTPS".into(),
            )
            .into());
        }
        changes.push(("tls_enabled", enabled.to_string()));
    }
    let entries: Vec<(&str, &str)> = changes.iter().map(|(k, v)| (*k, v.as_str())).collect();
    repo.set_many(&entries).await?;
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
    let _guard = certificates::lock_mutation().await?;
    certificates::issue_local(&repo, &req.hostnames, true).await?;
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
    let status_report = tokio::time::timeout(std::time::Duration::from_secs(10), client.get_status())
        .await.map_err(|_| Error::Unavailable("Tailscale did not respond. Open Tailscale on the MBL server and reconnect.".into()))?
        .map_err(|_| Error::Unavailable("Install and sign in to Tailscale on the MBL server. Allow the MBL service to access Tailscale.".into()))?;
    if status_report.backend_state != "Running" {
        return Err(Error::Unavailable(
            "Open Tailscale on the MBL server and sign in or reconnect.".into(),
        )
        .into());
    }
    let domain = status_report
        .magic_dns_name
        .filter(|d| !d.is_empty())
        .ok_or_else(|| Error::Unavailable("Enable MagicDNS in Tailscale DNS settings.".into()))?
        .trim_end_matches('.')
        .to_owned();
    certificates::enqueue(
        &repo,
        &CertificateRequest { mode: "tailscale".into(), domain, email: String::new() },
    )
    .await?;
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
    let domain = certificates::domain_name(&req.domain)?;
    let email = req.email.unwrap_or_default().trim().to_owned();
    if !email.is_empty()
        && (email.len() > 254 || !email.contains('@') || email.chars().any(char::is_whitespace))
    {
        return Err(Error::Validation("Enter a valid contact email address".into()).into());
    }
    let repo = SettingsRepo::new(state.db.clone());
    let request = CertificateRequest { mode: "letsencrypt".into(), domain, email: email.clone() };
    let cert = req.cert_pem.unwrap_or_default();
    let key = req.key_pem.unwrap_or_default();
    if cert.trim().is_empty() && key.trim().is_empty() {
        if !req.accept_terms {
            return Err(Error::Validation(
                "Accept the Let's Encrypt subscriber agreement before requesting a certificate"
                    .into(),
            )
            .into());
        }
        certificates::enqueue(&repo, &request).await?;
    } else {
        if cert.trim().is_empty() || key.trim().is_empty() {
            return Err(Error::Validation(
                "Provide both the certificate chain and private key".into(),
            )
            .into());
        }
        let _guard = certificates::lock_mutation().await?;
        certificates::activate(&repo, &request, &cert, &key, false).await?;
    }
    repo.set("tls_acme_email", &email).await?;
    Ok(Json(status(&repo).await?))
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
