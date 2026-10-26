//! Reachability and Carrier-Grade NAT (CGNAT) detection (M6.10, PLAN §13.3/§13.5).
//!
//! Compares router WAN address (from UPnP/PCP or local interface) with public echo address:
//! - If WAN address is in `100.64.0.0/10` (RFC 6598 Shared Address Space) -> Confirmed CGNAT.
//! - If WAN address is private RFC 1918 and mismatches public echo -> Double-NAT or CGNAT.
//! - If WAN address matches public echo IP -> Direct public IPv4/IPv6 reachable.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use utoipa::ToSchema;

use crate::network::{NetworkClass, classify_ip};

/// Outcome of the CGNAT and external reachability inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReachabilityReport {
    pub is_cgnat: bool,
    pub is_direct_reachable: bool,
    pub detected_wan_ip: Option<String>,
    pub detected_public_ip: Option<String>,
    pub recommendation: RemoteRecommendation,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RemoteRecommendation {
    /// Recommended for all users behind CGNAT (Tailscale, Headscale, NetBird).
    Tailscale,
    /// Public IP available: Port forward or UPnP with ACME TLS.
    PortForwardWithTls,
    /// CGNAT with friends on smart TVs without VPN support: VPS relay (WireGuard + Caddy).
    VpsRelay,
}

/// Run CGNAT analysis comparing router WAN IP to external echo IP.
pub fn analyze_reachability(
    wan_ip: Option<IpAddr>,
    public_ip: Option<IpAddr>,
) -> ReachabilityReport {
    let wan_str = wan_ip.map(|i| i.to_string());
    let pub_str = public_ip.map(|i| i.to_string());

    // Case 1: WAN IP explicitly in 100.64.0.0/10 (RFC 6598 CGNAT range)
    if let Some(w) = wan_ip
        && classify_ip(w) == NetworkClass::Tailnet
    {
        return ReachabilityReport {
            is_cgnat: true,
            is_direct_reachable: false,
            detected_wan_ip: wan_str,
            detected_public_ip: pub_str,
            recommendation: RemoteRecommendation::Tailscale,
            details: "Carrier-Grade NAT detected: router WAN IP is in the 100.64.0.0/10 ISP shared range. Port forwarding is not possible. Tailscale is recommended."
                .into(),
        };
    }

    // Case 2: WAN IP is a private address (192.168.x.x, 10.x.x.x) and doesn't match public IP
    if let (Some(w), Some(p)) = (wan_ip, public_ip) {
        if classify_ip(w) == NetworkClass::Lan && classify_ip(p) == NetworkClass::Wan {
            return ReachabilityReport {
                is_cgnat: true,
                is_direct_reachable: false,
                detected_wan_ip: wan_str,
                detected_public_ip: pub_str,
                recommendation: RemoteRecommendation::Tailscale,
                details: "Double-NAT or CGNAT detected: router has private IP while internet echo sees a different public address. Tailscale is recommended."
                    .into(),
            };
        }

        // Case 3: Public match
        if w == p && classify_ip(p) == NetworkClass::Wan {
            return ReachabilityReport {
                is_cgnat: false,
                is_direct_reachable: true,
                detected_wan_ip: wan_str,
                detected_public_ip: pub_str,
                recommendation: RemoteRecommendation::PortForwardWithTls,
                details: "Direct public IP confirmed: router WAN matches public IP. Port forwarding or UPnP with ACME TLS is supported."
                    .into(),
            };
        }
    }

    // Case 4: Only public IP known
    if let Some(p) = public_ip
        && classify_ip(p) == NetworkClass::Wan
    {
        return ReachabilityReport {
            is_cgnat: false,
            is_direct_reachable: true,
            detected_wan_ip: None,
            detected_public_ip: pub_str,
            recommendation: RemoteRecommendation::Tailscale,
            details: "Public IPv4/IPv6 echo reachable. Tailscale recommended for household; port forward available for external friends."
                .into(),
        };
    }

    ReachabilityReport {
        is_cgnat: false,
        is_direct_reachable: false,
        detected_wan_ip: wan_str,
        detected_public_ip: pub_str,
        recommendation: RemoteRecommendation::Tailscale,
        details: "Reachability check inconclusive. Tailscale is the safest recommendation.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cgnat_detection_scenarios() {
        // CGNAT ISP shared range 100.64.0.0/10
        let cgnat_ip: IpAddr = "100.70.1.2".parse().unwrap();
        let pub_ip: IpAddr = "93.184.216.34".parse().unwrap();
        let rep1 = analyze_reachability(Some(cgnat_ip), Some(pub_ip));
        assert!(rep1.is_cgnat);
        assert!(!rep1.is_direct_reachable);
        assert_eq!(rep1.recommendation, RemoteRecommendation::Tailscale);

        // Direct public match
        let rep2 = analyze_reachability(Some(pub_ip), Some(pub_ip));
        assert!(!rep2.is_cgnat);
        assert!(rep2.is_direct_reachable);
        assert_eq!(rep2.recommendation, RemoteRecommendation::PortForwardWithTls);

        // Double NAT (LAN behind upstream CGNAT)
        let lan_ip: IpAddr = "192.168.1.1".parse().unwrap();
        let rep3 = analyze_reachability(Some(lan_ip), Some(pub_ip));
        assert!(rep3.is_cgnat);
        assert!(!rep3.is_direct_reachable);
    }
}
