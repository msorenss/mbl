//! Optional HTTPS listener driven by the certificate settings stored in the database.
//!
//! The task polls the settings every few seconds, so enabling HTTPS, changing the port, or
//! installing a renewed certificate takes effect without restarting the server.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use tokio_util::sync::CancellationToken;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT: u16 = 8443;

#[derive(Clone, PartialEq, Eq)]
struct Desired {
    port: u16,
    cert: String,
    key: String,
}

async fn desired(repo: &SettingsRepo) -> Option<Desired> {
    if repo.get("tls_enabled").await.ok().flatten().as_deref() != Some("true") {
        return None;
    }
    let cert = repo.get("tls_cert_pem").await.ok().flatten()?;
    let key = repo.get("tls_key_pem").await.ok().flatten()?;
    let port = repo
        .get("tls_port")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    Some(Desired { port, cert, key })
}

/// Run until `stop` is cancelled, keeping the HTTPS listener in sync with the settings.
pub async fn run(db: Db, router: Router, bind: String, stop: CancellationToken) {
    // Several crypto providers may be linked in; pick one explicitly so rustls never panics.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let repo = SettingsRepo::new(db);
    let mut active: Option<(Desired, Handle<SocketAddr>)> = None;
    let mut failed: Option<Desired> = None;
    loop {
        let want = desired(&repo).await;
        let unchanged = active.as_ref().map(|(d, _)| d) == want.as_ref();
        if !unchanged && failed.as_ref() != want.as_ref() {
            if let Some((_, handle)) = active.take() {
                handle.graceful_shutdown(Some(DRAIN_TIMEOUT));
            }
            failed = None;
            if let Some(want) = want {
                match start(&bind, &want, router.clone()).await {
                    Ok(handle) => active = Some((want, handle)),
                    Err(error) => {
                        tracing::warn!(%error, port = want.port, "HTTPS listener not started");
                        failed = Some(want);
                    }
                }
            }
        }
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(POLL_INTERVAL) => {}
        }
    }
    if let Some((_, handle)) = active {
        handle.graceful_shutdown(Some(DRAIN_TIMEOUT));
    }
}

async fn start(bind: &str, want: &Desired, router: Router) -> anyhow::Result<Handle<SocketAddr>> {
    let addr: SocketAddr = format!("{bind}:{}", want.port).parse()?;
    let config =
        RustlsConfig::from_pem(want.cert.clone().into_bytes(), want.key.clone().into_bytes())
            .await?;
    let handle = Handle::new();
    let server = axum_server::bind_rustls(addr, config)
        .handle(handle.clone())
        .serve(router.into_make_service_with_connect_info::<SocketAddr>());
    tokio::spawn(async move {
        if let Err(error) = server.await {
            tracing::warn!(%error, "HTTPS listener stopped");
        }
    });
    // Wait until the socket is bound (or the bind failed and no address is ever reported).
    match tokio::time::timeout(Duration::from_secs(3), handle.listening()).await {
        Ok(Some(bound)) => {
            tracing::info!(addr = %bound, "HTTPS listening");
            Ok(handle)
        }
        _ => {
            handle.shutdown();
            anyhow::bail!("could not bind {addr}")
        }
    }
}
