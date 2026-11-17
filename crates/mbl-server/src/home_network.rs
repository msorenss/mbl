//! Keeps the server's own public address known so that LAN clients using the public
//! hostname (NAT hairpin) are classified as LAN.

use std::net::IpAddr;
use std::time::Duration;

use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use tokio_util::sync::CancellationToken;

const REFRESH: Duration = Duration::from_secs(300);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Host part of a configured public URL (`https://host:port/path` → `host`).
fn url_host(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://").map_or(url.trim(), |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        authority.rsplit_once(':').map_or(authority, |(host, port)| {
            if port.bytes().all(|b| b.is_ascii_digit()) { host } else { authority }
        })
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

async fn hostnames(repo: &SettingsRepo, public_url: &str) -> Vec<String> {
    let mut names: Vec<String> = repo
        .get("tls_hostnames")
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .split(',')
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty() && !name.starts_with('*'))
        .collect();
    names.extend(url_host(public_url));
    names.sort();
    names.dedup();
    names
}

/// Resolve every name; `None` when nothing could be resolved (keep the previous answer).
async fn resolve(names: &[String]) -> Option<Vec<IpAddr>> {
    let mut found = Vec::new();
    let mut any = false;
    for name in names {
        if let Ok(ip) = name.parse::<IpAddr>() {
            found.push(ip);
            any = true;
            continue;
        }
        let lookup = tokio::net::lookup_host((name.as_str(), 443));
        if let Ok(Ok(addresses)) = tokio::time::timeout(LOOKUP_TIMEOUT, lookup).await {
            found.extend(addresses.map(|address| address.ip()));
            any = true;
        }
    }
    any.then_some(found)
}

pub async fn run(db: Db, public_url: String, stop: CancellationToken) {
    let repo = SettingsRepo::new(db);
    loop {
        let names = hostnames(&repo, &public_url).await;
        if names.is_empty() {
            mbl_remote::set_home_addresses([]);
        } else if let Some(addresses) = resolve(&names).await {
            let before = mbl_remote::home_addresses();
            mbl_remote::set_home_addresses(addresses);
            let after = mbl_remote::home_addresses();
            if before != after {
                tracing::info!(addresses = ?after, "Home network public address updated");
            }
        }
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(REFRESH) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::url_host;

    #[test]
    fn extracts_host_from_public_url() {
        assert_eq!(url_host("https://MBL.example.com:8443/x").as_deref(), Some("mbl.example.com"));
        assert_eq!(url_host("http://192.168.1.5:8420").as_deref(), Some("192.168.1.5"));
        assert_eq!(url_host("https://[2001:db8::1]:8443/").as_deref(), Some("2001:db8::1"));
        assert_eq!(url_host("mbl.example.com").as_deref(), Some("mbl.example.com"));
        assert_eq!(url_host(""), None);
    }
}
