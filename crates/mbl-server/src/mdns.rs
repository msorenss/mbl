//! DNS-SD advertisement (`_mbl._tcp.local.`) so LAN clients can discover the server.

use std::time::Duration;

use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tokio_util::sync::CancellationToken;

pub const SERVICE_TYPE: &str = "_mbl._tcp.local.";

/// Advertise the HTTP listener until `stop` is cancelled. Failures are logged, never fatal.
pub async fn run(db: Db, bind: String, port: u16, stop: CancellationToken) {
    if is_loopback_bind(&bind) {
        tracing::debug!(%bind, "mDNS advertisement skipped for loopback bind");
        return;
    }
    let server_id = match mbl_service::server_id::ensure_server_id(&db).await {
        Ok(id) => id,
        Err(error) => {
            tracing::warn!(%error, "mDNS advertisement skipped: no server id");
            return;
        }
    };
    let repo = SettingsRepo::new(db);
    let https_port = match repo.get("tls_enabled").await {
        Ok(Some(v)) if v == "true" => repo.get("tls_port").await.ok().flatten(),
        _ => None,
    };
    let advertised =
        tokio::task::spawn_blocking(move || register(&server_id, port, https_port.as_deref()))
            .await;
    let (daemon, fullname) = match advertised {
        Ok(Ok(v)) => v,
        Ok(Err(error)) => {
            tracing::warn!(%error, "mDNS advertisement unavailable");
            return;
        }
        Err(_) => return,
    };
    tracing::info!(service = %fullname, "advertising via mDNS");
    stop.cancelled().await;
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(rx) = daemon.unregister(&fullname) {
            let _ = rx.recv_timeout(Duration::from_secs(1));
        }
        if let Ok(rx) = daemon.shutdown() {
            let _ = rx.recv_timeout(Duration::from_secs(1));
        }
    })
    .await;
}

fn register(
    server_id: &str,
    port: u16,
    https_port: Option<&str>,
) -> Result<(ServiceDaemon, String), mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;
    let short = &server_id[..server_id.len().min(8)];
    let instance = format!("MBL {} ({short})", host_label());
    let host = format!("mbl-{short}.local.");
    let mut props = vec![
        ("id", server_id.to_string()),
        ("version", env!("CARGO_PKG_VERSION").to_string()),
        ("path", "/".to_string()),
    ];
    if let Some(p) = https_port {
        props.push(("https_port", p.to_string()));
    }
    let info =
        ServiceInfo::new(SERVICE_TYPE, &instance, &host, "", port, &props[..])?.enable_addr_auto();
    let fullname = info.get_fullname().to_string();
    daemon.register(info)?;
    Ok((daemon, fullname))
}

fn is_loopback_bind(bind: &str) -> bool {
    bind.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()) || bind == "localhost"
}

fn host_label() -> String {
    let raw = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_default();
    let label: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ' ')
        .take(40)
        .collect();
    if label.is_empty() { "server".into() } else { label }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_binds_are_not_advertised() {
        assert!(is_loopback_bind("127.0.0.1"));
        assert!(is_loopback_bind("::1"));
        assert!(is_loopback_bind("localhost"));
        assert!(!is_loopback_bind("0.0.0.0"));
        assert!(!is_loopback_bind("192.168.1.10"));
    }
}
