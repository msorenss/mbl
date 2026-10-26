//! Tailscale LocalAPI client (M6.6).
//!
//! Connects to `tailscaled` over a Unix domain socket (`/var/run/tailscale/tailscaled.sock` on Linux)
//! or Windows named pipe to inspect:
//! - Local tailnet IPs and MagicDNS FQDN
//! - Backend state (Running, Stopped, NeedsLogin)
//! - Active peers and their direct vs DERP-relayed connection status
//! - Whois user identity mapping for peer IPs

use mbl_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use utoipa::ToSchema;

pub const DEFAULT_LINUX_SOCKET: &str = "/var/run/tailscale/tailscaled.sock";
pub const DEFAULT_WINDOWS_PIPE: &str =
    r"\\.\pipe\ProtectedPrefix\Administrators\Tailscale\tailscaled-auth";

/// Current status of the Tailscale node on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleStatus {
    pub backend_state: String,
    pub magic_dns_name: Option<String>,
    pub tailnet_ips: Vec<String>,
    pub peers: Vec<TailscalePeer>,
}

/// A peer on the tailnet and whether the connection is direct or relayed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TailscalePeer {
    pub host_name: String,
    pub tailnet_ips: Vec<String>,
    pub os: String,
    pub cur_addr: Option<String>,
    pub relay: Option<String>,
    pub is_direct: bool,
}

/// Resolved identity of a tailnet caller from LocalAPI `whois`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleWhois {
    pub login_name: String,
    pub display_name: String,
    pub node_name: String,
    pub node_ips: Vec<String>,
    pub caps: Vec<String>,
}

/// Abstract transport to allow live Unix socket / Named pipe or mock in tests.
#[async_trait::async_trait]
pub trait TailscaleTransport: Send + Sync {
    async fn request(&self, path_and_query: &str) -> Result<String>;
}

/// Tailscale LocalAPI client.
#[derive(Clone)]
pub struct TailscaleClient {
    transport: Arc<dyn TailscaleTransport>,
}

impl std::fmt::Debug for TailscaleClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TailscaleClient").finish()
    }
}

impl TailscaleClient {
    pub fn new(transport: Arc<dyn TailscaleTransport>) -> Self {
        Self { transport }
    }

    /// Construct a client using the host's default platform socket path.
    pub fn from_platform_default() -> Self {
        #[cfg(windows)]
        let path = PathBuf::from(DEFAULT_WINDOWS_PIPE);
        #[cfg(not(windows))]
        let path = PathBuf::from(DEFAULT_LINUX_SOCKET);

        Self::from_socket_path(path)
    }

    pub fn from_socket_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self { transport: Arc::new(LocalSocketTransport { path }) }
    }

    /// Fetch Tailscale daemon status (`/localapi/v0/status`).
    pub async fn get_status(&self) -> Result<TailscaleStatus> {
        let json_str = self.transport.request("/localapi/v0/status").await?;
        parse_tailscale_status(&json_str)
    }

    /// Look up the identity of a peer IP address (`/localapi/v0/whois?addr=...`).
    pub async fn whois(&self, peer_addr: &str) -> Result<TailscaleWhois> {
        let query = format!("/localapi/v0/whois?addr={peer_addr}");
        let json_str = self.transport.request(&query).await?;
        parse_tailscale_whois(&json_str)
    }
}

/// Real Unix domain socket transport.
struct LocalSocketTransport {
    path: PathBuf,
}

#[async_trait::async_trait]
impl TailscaleTransport for LocalSocketTransport {
    async fn request(&self, path_and_query: &str) -> Result<String> {
        if !self.path.exists() {
            return Err(Error::Unavailable(format!(
                "Tailscale socket not found at {}",
                self.path.display()
            )));
        }

        #[cfg(unix)]
        {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::UnixStream;

            let mut stream = UnixStream::connect(&self.path)
                .await
                .map_err(|e| Error::Unavailable(format!("failed to connect to tailscaled: {e}")))?;

            let req = format!(
                "GET {path_and_query} HTTP/1.1\r\nHost: local-tailscaled.sock\r\nSec-Tailscale: localapi\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(req.as_bytes())
                .await
                .map_err(|e| Error::Internal(format!("socket write failed: {e}").into()))?;

            let mut response = Vec::new();
            stream
                .read_to_end(&mut response)
                .await
                .map_err(|e| Error::Internal(format!("socket read failed: {e}").into()))?;

            let resp_str = String::from_utf8_lossy(&response);
            if let Some((_headers, body)) = resp_str.split_once("\r\n\r\n") {
                Ok(body.to_string())
            } else {
                Ok(resp_str.to_string())
            }
        }

        #[cfg(not(unix))]
        {
            Err(Error::Unavailable(
                "Tailscale LocalAPI named pipe connection not yet initialized on this platform"
                    .into(),
            ))
        }
    }
}

fn parse_tailscale_status(json_str: &str) -> Result<TailscaleStatus> {
    let v: serde_json::Value = serde_json::from_str(json_str).map_err(mbl_core::Error::internal)?;

    let backend_state = v["BackendState"].as_str().unwrap_or("Unknown").to_string();
    let magic_dns_name = v["Self"]["DNSName"].as_str().map(|s| s.trim_end_matches('.').to_string());
    let mut tailnet_ips = Vec::new();
    if let Some(arr) = v["Self"]["TailscaleIPs"].as_array() {
        for ip in arr {
            if let Some(s) = ip.as_str() {
                tailnet_ips.push(s.to_string());
            }
        }
    }

    let mut peers = Vec::new();
    if let Some(peer_map) = v["Peer"].as_object() {
        for (_k, peer_val) in peer_map {
            let host_name = peer_val["HostName"].as_str().unwrap_or("unknown").to_string();
            let os = peer_val["OS"].as_str().unwrap_or("unknown").to_string();
            let cur_addr = peer_val["CurAddr"].as_str().map(|s| s.to_string());
            let relay = peer_val["Relay"].as_str().map(|s| s.to_string());
            let is_direct = cur_addr.is_some() && relay.is_none();

            let mut ips = Vec::new();
            if let Some(arr) = peer_val["TailscaleIPs"].as_array() {
                for ip in arr {
                    if let Some(s) = ip.as_str() {
                        ips.push(s.to_string());
                    }
                }
            }

            peers.push(TailscalePeer {
                host_name,
                tailnet_ips: ips,
                os,
                cur_addr,
                relay,
                is_direct,
            });
        }
    }

    Ok(TailscaleStatus { backend_state, magic_dns_name, tailnet_ips, peers })
}

fn parse_tailscale_whois(json_str: &str) -> Result<TailscaleWhois> {
    let v: serde_json::Value = serde_json::from_str(json_str).map_err(mbl_core::Error::internal)?;

    let login_name = v["UserProfile"]["LoginName"]
        .as_str()
        .or_else(|| v["UserProfile"]["DisplayName"].as_str())
        .ok_or_else(|| Error::NotFound)?
        .to_string();
    let display_name = v["UserProfile"]["DisplayName"].as_str().unwrap_or(&login_name).to_string();
    let node_name = v["Node"]["ComputedName"]
        .as_str()
        .or_else(|| v["Node"]["HostName"].as_str())
        .unwrap_or("unknown")
        .to_string();

    let mut node_ips = Vec::new();
    if let Some(arr) = v["Node"]["TailscaleIPs"].as_array() {
        for ip in arr {
            if let Some(s) = ip.as_str() {
                node_ips.push(s.to_string());
            }
        }
    }

    let mut caps = Vec::new();
    if let Some(arr) = v["CapMap"].as_object() {
        for k in arr.keys() {
            caps.push(k.clone());
        }
    }

    Ok(TailscaleWhois { login_name, display_name, node_name, node_ips, caps })
}

// -----------------------------------------------------------------------------
// Mock transport for tests
// -----------------------------------------------------------------------------

#[derive(Default)]
pub struct MockTailscaleTransport {
    pub status_response: RwLock<Option<String>>,
    pub whois_response: RwLock<Option<String>>,
}

#[async_trait::async_trait]
impl TailscaleTransport for MockTailscaleTransport {
    async fn request(&self, path_and_query: &str) -> Result<String> {
        if path_and_query.starts_with("/localapi/v0/status") {
            let res = self.status_response.read().await;
            res.clone().ok_or_else(|| Error::NotFound)
        } else if path_and_query.starts_with("/localapi/v0/whois") {
            let res = self.whois_response.read().await;
            res.clone().ok_or_else(|| Error::NotFound)
        } else {
            Err(Error::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_tailscale_status_and_whois() {
        let mock = Arc::new(MockTailscaleTransport::default());

        let status_json = r#"{
            "BackendState": "Running",
            "Self": {
                "DNSName": "mbl-server.tailnet.ts.net.",
                "TailscaleIPs": ["100.64.0.1", "fd7a:115c:a1e0::1"]
            },
            "Peer": {
                "peer-1": {
                    "HostName": "livingroom-tv",
                    "TailscaleIPs": ["100.64.0.2"],
                    "OS": "android",
                    "CurAddr": "192.168.1.150:41641",
                    "Relay": null
                },
                "peer-2": {
                    "HostName": "phone-remote",
                    "TailscaleIPs": ["100.64.0.3"],
                    "OS": "iOS",
                    "CurAddr": null,
                    "Relay": "fra"
                }
            }
        }"#;

        let whois_json = r#"{
            "UserProfile": {
                "LoginName": "marcus@example.com",
                "DisplayName": "Marcus S"
            },
            "Node": {
                "ComputedName": "livingroom-tv",
                "TailscaleIPs": ["100.64.0.2"]
            },
            "CapMap": {
                "mbl/admin": []
            }
        }"#;

        *mock.status_response.write().await = Some(status_json.to_string());
        *mock.whois_response.write().await = Some(whois_json.to_string());

        let client = TailscaleClient::new(mock);

        let status = client.get_status().await.unwrap();
        assert_eq!(status.backend_state, "Running");
        assert_eq!(status.magic_dns_name, Some("mbl-server.tailnet.ts.net".into()));
        assert_eq!(status.tailnet_ips, vec!["100.64.0.1", "fd7a:115c:a1e0::1"]);
        assert_eq!(status.peers.len(), 2);

        let tv = status.peers.iter().find(|p| p.host_name == "livingroom-tv").unwrap();
        assert!(tv.is_direct);
        assert_eq!(tv.relay, None);

        let phone = status.peers.iter().find(|p| p.host_name == "phone-remote").unwrap();
        assert!(!phone.is_direct);
        assert_eq!(phone.relay, Some("fra".into()));

        let whois = client.whois("100.64.0.2").await.unwrap();
        assert_eq!(whois.login_name, "marcus@example.com");
        assert_eq!(whois.display_name, "Marcus S");
        assert_eq!(whois.node_name, "livingroom-tv");
        assert!(whois.caps.contains(&"mbl/admin".into()));
    }
}
