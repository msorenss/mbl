//! Tailscale identity login and strict trust enforcement (M6.7, PLAN §13.4).
//!
//! Strict Trust Rules:
//! 1. `Tailscale-User-*` headers (e.g. `Tailscale-User-Login`, `Tailscale-User-Name`) forwarded
//!    by `tailscale serve` are trusted **ONLY** when the request arrives directly from loopback (`127.0.0.1` or `::1`).
//! 2. Any incoming request from a non-loopback source containing `Tailscale-User-*` headers has them
//!    explicitly rejected and stripped to prevent header spoofing attacks from the LAN or WAN.
//! 3. For direct peer connections originating from Tailscale IPs (`100.64.0.0/10` or `fd7a:115c:a1e0::/48`),
//!    identity is resolved via LocalAPI `whois`.

use mbl_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use crate::network::{NetworkClass, classify_ip};
use crate::tailscale::TailscaleClient;

/// Authenticated identity derived from Tailscale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TailscaleAuthIdentity {
    pub login_name: String,
    pub display_name: String,
    pub node_name: Option<String>,
}

/// Strict evaluation of proxy headers vs direct peer socket.
pub fn evaluate_tailscale_serve_headers(
    peer_ip: IpAddr,
    header_login: Option<&str>,
    header_name: Option<&str>,
) -> Result<Option<TailscaleAuthIdentity>> {
    let Some(login) = header_login else {
        return Ok(None);
    };

    let login = login.trim();
    if login.is_empty() {
        return Ok(None);
    }

    // STRICT RULE: Only trust Tailscale proxy headers from loopback.
    if !peer_ip.is_loopback() {
        tracing::warn!(
            peer_ip = %peer_ip,
            spoofed_login = %login,
            "security alert: rejected forged Tailscale-User-* header from non-loopback address"
        );
        return Err(Error::Forbidden);
    }

    let display_name = header_name.unwrap_or(login).trim().to_string();

    Ok(Some(TailscaleAuthIdentity { login_name: login.to_string(), display_name, node_name: None }))
}

/// Resolve identity from direct connection on tailnet using LocalAPI whois.
pub async fn resolve_tailnet_peer_identity(
    client: &TailscaleClient,
    peer_ip: IpAddr,
) -> Result<Option<TailscaleAuthIdentity>> {
    let class = classify_ip(peer_ip);
    if class != NetworkClass::Tailnet {
        return Ok(None);
    }

    let peer_str = peer_ip.to_string();
    let whois = client.whois(&peer_str).await?;

    Ok(Some(TailscaleAuthIdentity {
        login_name: whois.login_name,
        display_name: whois.display_name,
        node_name: Some(whois.node_name),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tailscale::MockTailscaleTransport;
    use std::sync::Arc;

    #[test]
    fn test_reject_spoofed_tailscale_headers_from_lan_or_wan() {
        let lan_ip: IpAddr = "192.168.1.50".parse().unwrap();
        let wan_ip: IpAddr = "8.8.8.8".parse().unwrap();
        let loopback_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let loopback_v6: IpAddr = "::1".parse().unwrap();

        // Attempt from LAN -> Forbidden
        let res_lan =
            evaluate_tailscale_serve_headers(lan_ip, Some("attacker@tailnet"), Some("Fake Name"));
        assert!(matches!(res_lan, Err(Error::Forbidden)));

        // Attempt from WAN -> Forbidden
        let res_wan = evaluate_tailscale_serve_headers(wan_ip, Some("attacker@tailnet"), None);
        assert!(matches!(res_wan, Err(Error::Forbidden)));

        // Valid from loopback -> Ok
        let res_loop =
            evaluate_tailscale_serve_headers(loopback_ip, Some("alice@tailnet"), Some("Alice"));
        assert_eq!(
            res_loop.unwrap(),
            Some(TailscaleAuthIdentity {
                login_name: "alice@tailnet".into(),
                display_name: "Alice".into(),
                node_name: None,
            })
        );

        let res_loop_v6 = evaluate_tailscale_serve_headers(loopback_v6, Some("bob@tailnet"), None);
        assert_eq!(
            res_loop_v6.unwrap(),
            Some(TailscaleAuthIdentity {
                login_name: "bob@tailnet".into(),
                display_name: "bob@tailnet".into(),
                node_name: None,
            })
        );
    }

    #[tokio::test]
    async fn test_resolve_tailnet_peer_identity_whois() {
        let mock = Arc::new(MockTailscaleTransport::default());
        let whois_json = r#"{
            "UserProfile": {
                "LoginName": "charlie@tailnet.ts.net",
                "DisplayName": "Charlie C"
            },
            "Node": {
                "ComputedName": "charlie-laptop",
                "TailscaleIPs": ["100.64.0.10"]
            }
        }"#;
        *mock.whois_response.write().await = Some(whois_json.to_string());
        let client = TailscaleClient::new(mock);

        let tailnet_ip: IpAddr = "100.64.0.10".parse().unwrap();
        let identity = resolve_tailnet_peer_identity(&client, tailnet_ip).await.unwrap();
        assert_eq!(
            identity,
            Some(TailscaleAuthIdentity {
                login_name: "charlie@tailnet.ts.net".into(),
                display_name: "Charlie C".into(),
                node_name: Some("charlie-laptop".into()),
            })
        );

        // Non-tailnet IP returns None without calling whois
        let lan_ip: IpAddr = "192.168.1.10".parse().unwrap();
        let no_identity = resolve_tailnet_peer_identity(&client, lan_ip).await.unwrap();
        assert_eq!(no_identity, None);
    }
}
