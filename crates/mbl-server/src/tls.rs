//! Optional HTTPS listener driven by the certificate settings stored in the database.
//!
//! The task polls the settings every few seconds, so enabling HTTPS, changing the port, or
//! installing a renewed certificate takes effect without restarting the server.
//!
//! Whatever is currently serving keeps serving until its replacement is known to work: a
//! certificate is validated before it is swapped in, a new port is bound before the old one is
//! released, and a settings read that fails changes nothing. The reason the listener is not in
//! the configured state is stored in `tls_listener_error` for the admin UI.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use anyhow::Context;
use axum::Router;
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use tokio_util::sync::CancellationToken;

const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_PORT: u16 = 8443;
const ERROR_KEY: &str = "tls_listener_error";

/// How often the listener compares itself with the settings.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Delay between two looks at the settings.
    pub poll: Duration,
    /// Delay before retrying a configuration that could not be applied, such as a busy port.
    pub retry: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self { poll: Duration::from_secs(5), retry: Duration::from_secs(30) }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Desired {
    port: u16,
    cert: String,
    key: String,
}

struct HttpsListener {
    config: RustlsConfig,
    handle: Handle<SocketAddr>,
    task: tokio::task::JoinHandle<()>,
}

impl HttpsListener {
    async fn stop(self) {
        self.handle.graceful_shutdown(Some(DRAIN_TIMEOUT));
        let _ = self.task.await;
    }
}

/// The configuration the settings ask for, or `None` when HTTPS is switched off. A read error
/// is returned as an error so that it is never mistaken for "switched off".
async fn desired(repo: &SettingsRepo) -> mbl_core::Result<Option<Desired>> {
    if repo.get("tls_enabled").await?.as_deref() != Some("true") {
        return Ok(None);
    }
    let cert = repo.get("tls_cert_pem").await?.filter(|v| !v.is_empty());
    let key = repo.get("tls_key_pem").await?.filter(|v| !v.is_empty());
    let (Some(cert), Some(key)) = (cert, key) else {
        return Ok(None);
    };
    let port = repo.get("tls_port").await?.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_PORT);
    Ok(Some(Desired { port, cert, key }))
}

struct Supervisor {
    repo: SettingsRepo,
    router: Router,
    bind: String,
    timing: Timing,
    active: Option<(Desired, HttpsListener)>,
    /// The last configuration that could not be applied, and when it was tried.
    failed: Option<(Desired, Instant)>,
    /// The error text last written to the settings, to avoid rewriting it on every poll.
    reported: Option<String>,
}

impl Supervisor {
    async fn sync(&mut self) {
        let want = match desired(&self.repo).await {
            Ok(want) => want,
            Err(error) => {
                tracing::warn!(%error, "HTTPS settings unreadable; keeping the current listener");
                return;
            }
        };
        // A listener whose task ended on its own is no longer serving.
        if self.active.as_ref().is_some_and(|(_, listener)| listener.task.is_finished()) {
            tracing::warn!("HTTPS listener stopped unexpectedly; restarting");
            self.active = None;
        }
        if self.active.as_ref().map(|(current, _)| current) == want.as_ref() {
            self.failed = None;
            self.report("").await;
            return;
        }
        let Some(want) = want else {
            if let Some((_, listener)) = self.active.take() {
                listener.stop().await;
                tracing::info!("HTTPS listener stopped");
            }
            self.failed = None;
            self.report("").await;
            return;
        };
        if self
            .failed
            .as_ref()
            .is_some_and(|(bad, at)| *bad == want && at.elapsed() < self.timing.retry)
        {
            return;
        }
        match self.apply(&want).await {
            Ok(()) => {
                self.failed = None;
                self.report("").await;
            }
            Err(error) => {
                let message = format!("{error:#}");
                tracing::warn!(error = %message, port = want.port, "HTTPS configuration not applied");
                self.failed = Some((want, Instant::now()));
                self.report(&message).await;
            }
        }
    }

    /// Move to `want` without interrupting a working listener unless the replacement works.
    async fn apply(&mut self, want: &Desired) -> anyhow::Result<()> {
        let cert = want.cert.clone().into_bytes();
        let key = want.key.clone().into_bytes();
        if let Some((current, listener)) = self.active.as_mut()
            && current.port == want.port
        {
            // Swaps the certificate for new connections only when it parses and matches its
            // key; established connections, such as running streams, are untouched.
            listener
                .config
                .reload_from_pem(cert, key)
                .await
                .context("the certificate or private key is not usable")?;
            *current = want.clone();
            tracing::info!("HTTPS certificate reloaded");
            return Ok(());
        }
        let config = RustlsConfig::from_pem(cert, key)
            .await
            .context("the certificate or private key is not usable")?;
        let listener = start(&self.bind, want.port, config, self.router.clone()).await?;
        if let Some((_, previous)) = self.active.replace((want.clone(), listener)) {
            previous.stop().await;
        }
        Ok(())
    }

    async fn report(&mut self, message: &str) {
        if self.reported.as_deref() == Some(message) {
            return;
        }
        match self.repo.set(ERROR_KEY, message).await {
            Ok(()) => self.reported = Some(message.to_owned()),
            Err(error) => tracing::warn!(%error, "HTTPS listener status not saved"),
        }
    }
}

/// Run until `stop` is cancelled, keeping the HTTPS listener in sync with the settings.
pub async fn run(db: Db, router: Router, bind: String, stop: CancellationToken) {
    run_with(db, router, bind, stop, Timing::default()).await;
}

/// [`run`] with explicit timing.
pub async fn run_with(
    db: Db,
    router: Router,
    bind: String,
    stop: CancellationToken,
    timing: Timing,
) {
    // Several crypto providers may be linked in; pick one explicitly so rustls never panics.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut supervisor = Supervisor {
        repo: SettingsRepo::new(db),
        router,
        bind,
        timing,
        active: None,
        failed: None,
        reported: None,
    };
    loop {
        supervisor.sync().await;
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(timing.poll) => {}
        }
    }
    if let Some((_, listener)) = supervisor.active {
        listener.stop().await;
    }
}

/// Resolve the configured bind host, which may be an IPv4 or IPv6 literal or a host name.
async fn bind_addr(bind: &str, port: u16) -> anyhow::Result<SocketAddr> {
    let host = bind.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    tokio::net::lookup_host((host, port))
        .await
        .with_context(|| format!("cannot resolve bind address {bind}"))?
        .next()
        .with_context(|| format!("bind address {bind} has no usable address"))
}

async fn start(
    bind: &str,
    port: u16,
    config: RustlsConfig,
    router: Router,
) -> anyhow::Result<HttpsListener> {
    let addr = bind_addr(bind, port).await?;
    // Bind here so the real reason for a failure (port in use, permission) is reported.
    let socket =
        std::net::TcpListener::bind(addr).with_context(|| format!("cannot listen on {addr}"))?;
    socket.set_nonblocking(true)?;
    let handle = Handle::new();
    // Everything served here is encrypted; handlers use the marker to set `Secure` cookies.
    let router = router.layer(axum::Extension(mbl_api::extract::SecureTransport));
    let server = axum_server::from_tcp_rustls(socket, config.clone())?
        .handle(handle.clone())
        .serve(router.into_make_service_with_connect_info::<SocketAddr>());
    let task = tokio::spawn(async move {
        if let Err(error) = server.await {
            tracing::warn!(%error, "HTTPS listener stopped");
        }
    });
    match tokio::time::timeout(Duration::from_secs(3), handle.listening()).await {
        Ok(Some(bound)) => {
            tracing::info!(addr = %bound, "HTTPS listening");
            Ok(HttpsListener { config, handle, task })
        }
        _ => {
            handle.shutdown();
            let _ = task.await;
            anyhow::bail!("HTTPS listener on {addr} did not start")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_addresses_accept_ip_literals_of_both_families() {
        assert_eq!(bind_addr("0.0.0.0", 8443).await.unwrap().to_string(), "0.0.0.0:8443");
        assert_eq!(bind_addr("::", 8443).await.unwrap().to_string(), "[::]:8443");
        assert_eq!(bind_addr("[::1]", 9000).await.unwrap().to_string(), "[::1]:9000");
        assert!(bind_addr("not a host", 8443).await.is_err());
    }
}
