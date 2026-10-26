//! Remote bandwidth budget, fair share allocation, and client throughput feedback (M6.5, PLAN §13.7).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use utoipa::ToSchema;

use crate::network::NetworkClass;

/// Default global WAN upload cap in kbps (20 Mbps = 20_000 kbps).
pub const DEFAULT_WAN_UPLOAD_CAP_KBPS: u64 = 20_000;
/// Minimum allocation per WAN stream to ensure usable 720p/1080p video (1.5 Mbps).
pub const MIN_WAN_STREAM_KBPS: u64 = 1_500;

/// Quality policy and bitrate recommendation per network type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQualityPreset {
    pub network: NetworkClass,
    pub max_bitrate_kbps: Option<u64>,
    pub prefer_hw_hevc: bool,
    pub description: &'static str,
}

impl NetworkQualityPreset {
    pub fn for_network(net: NetworkClass) -> Self {
        match net {
            NetworkClass::Lan => Self {
                network: net,
                max_bitrate_kbps: None, // Original / Unlimited
                prefer_hw_hevc: false,
                description: "Direct Play with original bitrates on local network",
            },
            NetworkClass::Tailnet => Self {
                network: net,
                max_bitrate_kbps: Some(25_000), // High / 25 Mbps
                prefer_hw_hevc: true,
                description: "High quality peer-to-peer over Tailscale mesh",
            },
            NetworkClass::Wan => Self {
                network: net,
                max_bitrate_kbps: Some(8_000), // Standard WAN 1080p
                prefer_hw_hevc: true,
                description: "Bandwidth-constrained remote stream with adaptive bitrate",
            },
        }
    }
}

/// Recorded feedback from client playback progress reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientThroughputFeedback {
    pub session_id: String,
    pub measured_bitrate_kbps: Option<u64>,
    pub buffer_health_secs: Option<u64>,
    pub last_updated_at: i64,
}

/// Manages active remote streams and calculates fair-share bandwidth.
pub struct BandwidthManager {
    global_wan_limit_kbps: RwLock<u64>,
    active_wan_sessions: RwLock<HashMap<String, ClientThroughputFeedback>>,
}

impl std::fmt::Debug for BandwidthManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BandwidthManager").finish()
    }
}

impl Default for BandwidthManager {
    fn default() -> Self {
        Self::new(DEFAULT_WAN_UPLOAD_CAP_KBPS)
    }
}

impl BandwidthManager {
    pub fn new(global_wan_limit_kbps: u64) -> Self {
        Self {
            global_wan_limit_kbps: RwLock::new(global_wan_limit_kbps),
            active_wan_sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_wan_limit(&self, limit_kbps: u64) {
        if let Ok(mut l) = self.global_wan_limit_kbps.write() {
            *l = limit_kbps;
        }
    }

    pub fn get_wan_limit(&self) -> u64 {
        self.global_wan_limit_kbps.read().map(|l| *l).unwrap_or(DEFAULT_WAN_UPLOAD_CAP_KBPS)
    }

    pub fn register_session(&self, session_id: &str) {
        if let Ok(mut map) = self.active_wan_sessions.write() {
            map.insert(
                session_id.to_string(),
                ClientThroughputFeedback {
                    session_id: session_id.to_string(),
                    measured_bitrate_kbps: None,
                    buffer_health_secs: None,
                    last_updated_at: mbl_core::time::now_ms(),
                },
            );
        }
    }

    pub fn unregister_session(&self, session_id: &str) {
        if let Ok(mut map) = self.active_wan_sessions.write() {
            map.remove(session_id);
        }
    }

    pub fn record_feedback(
        &self,
        session_id: &str,
        measured_bitrate_kbps: Option<u64>,
        buffer_health_secs: Option<u64>,
    ) {
        if let Ok(mut map) = self.active_wan_sessions.write()
            && let Some(entry) = map.get_mut(session_id)
        {
            if measured_bitrate_kbps.is_some() {
                entry.measured_bitrate_kbps = measured_bitrate_kbps;
            }
            if buffer_health_secs.is_some() {
                entry.buffer_health_secs = buffer_health_secs;
            }
            entry.last_updated_at = mbl_core::time::now_ms();
        }
    }

    /// Calculate recommended bitrate for a WAN session based on active fair-share.
    pub fn calculate_wan_fair_share(&self, requested_max: Option<u64>) -> u64 {
        let active_count =
            self.active_wan_sessions.read().map(|m| m.len() as u64).unwrap_or(1).max(1);

        let total_cap = self.get_wan_limit();
        let per_stream_share = (total_cap / active_count).max(MIN_WAN_STREAM_KBPS);

        match requested_max {
            Some(req) => req.min(per_stream_share),
            None => per_stream_share,
        }
    }

    pub fn active_session_count(&self) -> usize {
        self.active_wan_sessions.read().map(|m| m.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fair_share_allocation() {
        let mgr = BandwidthManager::new(20_000); // 20 Mbps

        // Single session gets the full budget (capped by requested if any)
        assert_eq!(mgr.calculate_wan_fair_share(None), 20_000);
        assert_eq!(mgr.calculate_wan_fair_share(Some(8_000)), 8_000);

        // 2 sessions -> 10 Mbps each
        mgr.register_session("sess-1");
        mgr.register_session("sess-2");
        assert_eq!(mgr.calculate_wan_fair_share(None), 10_000);
        assert_eq!(mgr.calculate_wan_fair_share(Some(12_000)), 10_000);

        // 4 sessions -> 5 Mbps each
        mgr.register_session("sess-3");
        mgr.register_session("sess-4");
        assert_eq!(mgr.calculate_wan_fair_share(None), 5_000);

        mgr.unregister_session("sess-4");
        assert_eq!(mgr.active_session_count(), 3);
    }

    #[test]
    fn test_network_quality_presets() {
        let lan = NetworkQualityPreset::for_network(NetworkClass::Lan);
        assert_eq!(lan.max_bitrate_kbps, None);
        assert!(!lan.prefer_hw_hevc);

        let tailnet = NetworkQualityPreset::for_network(NetworkClass::Tailnet);
        assert_eq!(tailnet.max_bitrate_kbps, Some(25_000));
        assert!(tailnet.prefer_hw_hevc);

        let wan = NetworkQualityPreset::for_network(NetworkClass::Wan);
        assert_eq!(wan.max_bitrate_kbps, Some(8_000));
        assert!(wan.prefer_hw_hevc);
    }
}
