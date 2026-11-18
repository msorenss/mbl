use std::net::IpAddr;
use std::sync::Arc;

use mbl_core::{Actor, Result};
use mbl_db::Db;
use mbl_db::repo::m6::{get_remote_setting, set_remote_setting};
use std::time::Duration;

use mbl_db::repo::settings::SettingsRepo;
use mbl_remote::{
    BandwidthManager, CheckStatus, ReachabilityCheck, ReachabilityReport, RemoteRecommendation,
    TailscaleClient, TailscaleStatus, analyze_reachability,
};

use crate::remote_probe;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatusResponse {
    pub tailscale: Option<TailscaleStatus>,
    pub is_admin_lan_only: bool,
    pub is_tailscale_auth_enabled: bool,
    pub wan_upload_limit_mbps: u64,
    pub active_wan_sessions: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRemotePolicyRequest {
    pub admin_lan_only: Option<bool>,
    pub tailscale_auth_enabled: Option<bool>,
    pub wan_upload_limit_mbps: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthUsageResponse {
    pub global_upload_limit_kbps: u64,
    pub active_wan_sessions: usize,
    pub per_stream_fair_share_kbps: u64,
}

#[derive(Clone, Debug)]
pub struct RemoteService {
    db: Db,
    tailscale: Arc<TailscaleClient>,
    bandwidth: Arc<BandwidthManager>,
    public_url: String,
    /// Run the network probes (public echo, UPnP, DNS, port). Off in tests.
    probes: bool,
}

impl RemoteService {
    pub fn new(db: Db, tailscale: Arc<TailscaleClient>, bandwidth: Arc<BandwidthManager>) -> Self {
        Self { db, tailscale, bandwidth, public_url: String::new(), probes: true }
    }

    /// Configured `server.public_url`, used to find the public host name.
    pub fn with_public_url(mut self, public_url: impl Into<String>) -> Self {
        self.public_url = public_url.into();
        self
    }

    /// Replace the Tailscale client, keeping the other settings.
    pub fn with_tailscale(mut self, client: Arc<TailscaleClient>) -> Self {
        self.tailscale = client;
        self
    }

    /// Disable the outbound network probes (tests and offline installs).
    pub fn without_probes(mut self) -> Self {
        self.probes = false;
        self
    }

    pub async fn restore_policy(&self) -> Result<()> {
        let mbps = get_remote_setting(&self.db, "wan_upload_limit_mbps")
            .await?
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(20);
        self.bandwidth.set_wan_limit(mbps.saturating_mul(1000));
        Ok(())
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn bandwidth(&self) -> &Arc<BandwidthManager> {
        &self.bandwidth
    }

    pub fn tailscale(&self) -> &Arc<TailscaleClient> {
        &self.tailscale
    }

    /// Read comprehensive remote access status.
    pub async fn get_status(&self, actor: &Actor) -> Result<RemoteStatusResponse> {
        actor.require_admin()?;

        let ts_status = self.tailscale.get_status().await.ok();
        let admin_lan_only = get_remote_setting(&self.db, "admin_lan_only")
            .await?
            .map(|s| s == "true" || s == "1")
            .unwrap_or(true); // default true per PLAN §13.5

        let ts_auth = get_remote_setting(&self.db, "tailscale_auth_enabled")
            .await?
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false);

        let upload_mbps = get_remote_setting(&self.db, "wan_upload_limit_mbps")
            .await?
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(20);

        Ok(RemoteStatusResponse {
            tailscale: ts_status,
            is_admin_lan_only: admin_lan_only,
            is_tailscale_auth_enabled: ts_auth,
            wan_upload_limit_mbps: upload_mbps,
            active_wan_sessions: self.bandwidth.active_session_count(),
        })
    }

    /// Run the remote-access diagnostic: public address, router WAN address (UPnP), DNS of
    /// the public host names, a TCP check of the HTTPS port, certificate and Tailscale.
    pub async fn test_reachability(
        &self,
        actor: &Actor,
        public_ip_override: Option<String>,
    ) -> Result<ReachabilityReport> {
        actor.require_admin()?;
        let settings = SettingsRepo::new(self.db.clone());
        let tls_enabled = settings.get("tls_enabled").await?.as_deref() == Some("true");
        let tls_mode = settings.get("tls_mode").await?.unwrap_or_else(|| "none".into());
        let has_cert = settings.get("tls_cert_pem").await?.is_some_and(|c| !c.is_empty());
        let expires_at: Option<i64> =
            settings.get("tls_cert_expires_at").await?.and_then(|v| v.parse().ok());
        let https_port: u16 =
            settings.get("tls_port").await?.and_then(|v| v.parse().ok()).unwrap_or(8443);
        let mut hostnames: Vec<String> = settings
            .get("tls_hostnames")
            .await?
            .unwrap_or_default()
            .split(',')
            .map(|n| n.trim().to_ascii_lowercase())
            .filter(|n| !n.is_empty() && !n.starts_with('*') && !n.ends_with(".ts.net"))
            .filter(|n| {
                n.parse::<IpAddr>()
                    .map_or(true, |ip| mbl_remote::classify_ip(ip) == mbl_remote::NetworkClass::Wan)
            })
            .collect();
        hostnames.extend(mbl_remote::url_host(&self.public_url).filter(|h| {
            h.parse::<IpAddr>()
                .map_or(true, |ip| mbl_remote::classify_ip(ip) == mbl_remote::NetworkClass::Wan)
        }));
        hostnames.sort();
        hostnames.dedup();

        let override_ip: Option<IpAddr> =
            match public_ip_override.as_deref().map(str::trim) {
                Some("") | None => None,
                Some(s) => Some(s.parse().map_err(|_| {
                    mbl_core::Error::Validation(format!("'{s}' is not an IP address"))
                })?),
            };
        let probe_timeout = Duration::from_secs(4);
        let (echo_ip, wan_ip, tailscale) = if self.probes {
            tokio::join!(
                async {
                    match override_ip {
                        Some(ip) => Some(ip),
                        None => remote_probe::public_ip(probe_timeout).await,
                    }
                },
                remote_probe::upnp_external_ip(Duration::from_secs(3)),
                self.tailscale.get_status(),
            )
        } else {
            (override_ip, None, self.tailscale.get_status().await)
        };

        let mut report = analyze_reachability(wan_ip, echo_ip);
        let mut checks = Vec::new();

        // Public address.
        checks.push(match echo_ip {
            Some(ip) if mbl_remote::classify_ip(ip) == mbl_remote::NetworkClass::Wan => {
                ReachabilityCheck::new("publicIp", CheckStatus::Ok, format!("The internet sees this server as {ip}."))
            }
            Some(ip) => ReachabilityCheck::new("publicIp", CheckStatus::Fail, format!("{ip} is not a public address.")),
            None if !self.probes => ReachabilityCheck::new("publicIp", CheckStatus::Skipped, "Network probes are disabled."),
            None => ReachabilityCheck::new("publicIp", CheckStatus::Fail, format!("Could not reach {} to learn the public address. Check the server's internet connection.", remote_probe::PUBLIC_IP_ECHO)),
        });

        // Router WAN address.
        checks.push(match (wan_ip, echo_ip) {
            (Some(w), _) if mbl_remote::classify_ip(w) == mbl_remote::NetworkClass::Tailnet => ReachabilityCheck::new(
                "routerWan",
                CheckStatus::Fail,
                format!("The router's WAN address {w} is in the ISP shared range (CGNAT). Port forwarding cannot work; use Tailscale or ask the ISP for a public IPv4 address."),
            ),
            (Some(w), Some(p)) if w == p => ReachabilityCheck::new("routerWan", CheckStatus::Ok, format!("The router has the public address {w} itself, so port forwarding can work.")),
            (Some(w), Some(p)) => ReachabilityCheck::new(
                "routerWan",
                CheckStatus::Fail,
                format!("The router's WAN address {w} differs from the public address {p} (double NAT or CGNAT). Port forwarding on this router is not enough."),
            ),
            (Some(w), None) => ReachabilityCheck::new("routerWan", CheckStatus::Warn, format!("The router reports WAN address {w}, but the public address is unknown.")),
            (None, _) if !self.probes => ReachabilityCheck::new("routerWan", CheckStatus::Skipped, "Network probes are disabled."),
            (None, _) => ReachabilityCheck::new(
                "routerWan",
                CheckStatus::Skipped,
                "The router did not answer UPnP, so its WAN address is unknown. Compare the WAN address in the router's admin page with the public address.",
            ),
        });

        // DNS of the public host names.
        let mut dns_matches = false;
        if hostnames.is_empty() {
            checks.push(ReachabilityCheck::new(
                "dns",
                CheckStatus::Skipped,
                "No public host name is configured. Set server.public_url or request a Let's Encrypt certificate for a host name.",
            ));
        } else if self.probes {
            for name in &hostnames {
                let ips = remote_probe::resolve(name, probe_timeout).await;
                let list = ips.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                checks.push(match echo_ip {
                    _ if ips.is_empty() => ReachabilityCheck::new("dns", CheckStatus::Fail, format!("{name} does not resolve.")),
                    Some(p) if ips.contains(&p) => {
                        dns_matches = true;
                        ReachabilityCheck::new("dns", CheckStatus::Ok, format!("{name} points to {p}, the public address."))
                    }
                    Some(p) => ReachabilityCheck::new(
                        "dns",
                        CheckStatus::Fail,
                        format!("{name} points to {list}, but the public address is {p}. Update the DNS record or dynamic DNS client."),
                    ),
                    None => ReachabilityCheck::new("dns", CheckStatus::Warn, format!("{name} points to {list}; the public address is unknown.")),
                });
            }
        }

        // Port check (needs NAT loopback on the router when run from inside).
        let mut port_open = None;
        if !tls_enabled || !has_cert {
            checks.push(ReachabilityCheck::new(
                "port",
                CheckStatus::Skipped,
                "HTTPS is off, so there is no port to forward. Turn on HTTPS before opening the server to the internet.",
            ));
        } else if let (true, Some(ip)) = (
            self.probes,
            echo_ip.filter(|ip| mbl_remote::classify_ip(*ip) == mbl_remote::NetworkClass::Wan),
        ) {
            let open = remote_probe::tcp_open(ip, https_port, Duration::from_secs(3)).await;
            port_open = Some(open);
            checks.push(if open {
                ReachabilityCheck::new("port", CheckStatus::Ok, format!("{ip}:{https_port} accepts connections; the port forward works."))
            } else {
                ReachabilityCheck::new(
                    "port",
                    CheckStatus::Warn,
                    format!("Could not connect to {ip}:{https_port} from inside the network. Either the port forward to this server is missing, or the router does not support NAT loopback; test from mobile data to be sure."),
                )
            });
            if tls_mode == "letsencrypt" {
                let http_open = remote_probe::tcp_open(ip, 80, Duration::from_secs(3)).await;
                checks.push(if http_open {
                    ReachabilityCheck::new("port80", CheckStatus::Ok, format!("{ip}:80 accepts connections, so Let's Encrypt renewals can reach the server."))
                } else {
                    ReachabilityCheck::new(
                        "port80",
                        CheckStatus::Warn,
                        format!("Could not connect to {ip}:80. Let's Encrypt renewal needs external port 80 forwarded to this server (port 8081, or through a reverse proxy)."),
                    )
                });
            }
        }

        // Certificate.
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        checks.push(match (tls_enabled, has_cert, expires_at) {
            (false, _, _) => ReachabilityCheck::new("certificate", CheckStatus::Warn, "HTTPS is off. Remote clients would send passwords unencrypted."),
            (true, false, _) => ReachabilityCheck::new("certificate", CheckStatus::Fail, "HTTPS is on but no certificate is installed."),
            (true, true, Some(exp)) if exp <= now => ReachabilityCheck::new("certificate", CheckStatus::Fail, "The certificate has expired."),
            (true, true, Some(exp)) if exp - now < 14 * 86_400 => ReachabilityCheck::new(
                "certificate",
                CheckStatus::Warn,
                format!("The {tls_mode} certificate expires in {} days.", (exp - now) / 86_400),
            ),
            (true, true, _) if tls_mode == "local_ca" => ReachabilityCheck::new(
                "certificate",
                CheckStatus::Warn,
                "The certificate comes from MBL's local CA. Devices outside the home must install the CA; Let's Encrypt avoids that.",
            ),
            (true, true, exp) => ReachabilityCheck::new(
                "certificate",
                CheckStatus::Ok,
                match exp {
                    Some(exp) => format!("A valid {tls_mode} certificate is installed ({} days left).", (exp - now) / 86_400),
                    None => format!("A {tls_mode} certificate is installed."),
                },
            ),
        });

        // Tailscale.
        checks.push(match &tailscale {
            Ok(status) if status.backend_state == "Running" => ReachabilityCheck::new(
                "tailscale",
                CheckStatus::Ok,
                match &status.magic_dns_name {
                    Some(name) => format!(
                        "Tailscale is running; devices on the tailnet reach the server at {}.",
                        name.trim_end_matches('.')
                    ),
                    None => "Tailscale is running.".to_owned(),
                },
            ),
            Ok(status) => ReachabilityCheck::new(
                "tailscale",
                CheckStatus::Warn,
                format!("Tailscale is installed but {}.", status.backend_state),
            ),
            Err(_) => ReachabilityCheck::new(
                "tailscale",
                CheckStatus::Skipped,
                "Tailscale is not installed or not reachable.",
            ),
        });

        // Verdict: an open port with a public address is the only real proof of direct access.
        if report.is_cgnat {
            report.is_direct_reachable = false;
        } else if port_open == Some(true) {
            report.is_direct_reachable = true;
            report.recommendation = RemoteRecommendation::PortForwardWithTls;
            report.details = if dns_matches || hostnames.is_empty() {
                "The server is reachable from the internet over HTTPS.".into()
            } else {
                "The HTTPS port is open, but the public host name does not point to this address."
                    .into()
            };
        } else if report.is_direct_reachable {
            report.is_direct_reachable = false;
            report.details = "The public address is direct (no CGNAT), but the HTTPS port could not be confirmed open. Forward the HTTPS port to this server, or use Tailscale.".into();
        }

        report.checks = checks;
        report.public_hostnames = hostnames;
        report.https_port = tls_enabled.then_some(https_port);
        Ok(report)
    }

    /// Update remote access policies (admin LAN-only, Tailscale auth, WAN upload cap).
    pub async fn update_policy(
        &self,
        actor: &Actor,
        req: UpdateRemotePolicyRequest,
    ) -> Result<RemoteStatusResponse> {
        actor.require_admin()?;

        if let Some(lan_only) = req.admin_lan_only {
            set_remote_setting(&self.db, "admin_lan_only", if lan_only { "true" } else { "false" })
                .await?;
        }
        if let Some(ts_auth) = req.tailscale_auth_enabled {
            set_remote_setting(
                &self.db,
                "tailscale_auth_enabled",
                if ts_auth { "true" } else { "false" },
            )
            .await?;
        }
        if let Some(limit_mbps) = req.wan_upload_limit_mbps {
            set_remote_setting(&self.db, "wan_upload_limit_mbps", &limit_mbps.to_string()).await?;
            self.bandwidth.set_wan_limit(limit_mbps.saturating_mul(1_000));
        }

        self.get_status(actor).await
    }

    /// Fetch active bandwidth allocation statistics.
    pub async fn get_bandwidth_usage(&self, actor: &Actor) -> Result<BandwidthUsageResponse> {
        actor.require_admin()?;

        let total_kbps = self.bandwidth.get_wan_limit();
        let sessions = self.bandwidth.active_session_count();
        let fair_share = self.bandwidth.calculate_wan_fair_share(None);

        Ok(BandwidthUsageResponse {
            global_upload_limit_kbps: total_kbps,
            active_wan_sessions: sessions,
            per_stream_fair_share_kbps: fair_share,
        })
    }
}
