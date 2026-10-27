use std::net::IpAddr;
use std::sync::Arc;

use mbl_core::{Actor, Result};
use mbl_db::Db;
use mbl_db::repo::m6::{get_remote_setting, set_remote_setting};
use mbl_remote::{
    BandwidthManager, ReachabilityReport, TailscaleClient, TailscaleStatus, analyze_reachability,
};
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
}

impl RemoteService {
    pub fn new(db: Db, tailscale: Arc<TailscaleClient>, bandwidth: Arc<BandwidthManager>) -> Self {
        Self { db, tailscale, bandwidth }
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

    /// Run reachability and CGNAT diagnostic test.
    pub async fn test_reachability(
        &self,
        actor: &Actor,
        public_ip_override: Option<String>,
    ) -> Result<ReachabilityReport> {
        actor.require_admin()?;

        let pub_ip: Option<IpAddr> = match public_ip_override {
            Some(s) => s.parse().ok(),
            None => None,
        };

        let mut wan_ip: Option<IpAddr> = None;
        if let Ok(status) = self.tailscale.get_status().await
            && let Some(first_ip) = status.tailnet_ips.first()
        {
            wan_ip = first_ip.parse().ok();
        }

        Ok(analyze_reachability(wan_ip, pub_ip))
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
