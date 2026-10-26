//! Network classification and trusted proxy client resolution (M6.4).
//!
//! Requests are classified into:
//! - `Lan`: RFC 1918 private IPv4 subnets, IPv6 ULA, loopback, and link-local.
//! - `Tailnet`: Tailscale Carrier-Grade NAT range `100.64.0.0/10` and IPv6 ULA `fd7a:115c:a1e0::/48`.
//! - `Wan`: All other public internet addresses.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use utoipa::ToSchema;

/// The network environment from which a request originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NetworkClass {
    Lan,
    Tailnet,
    Wan,
}

impl NetworkClass {
    pub fn is_lan(&self) -> bool {
        matches!(self, Self::Lan)
    }

    pub fn is_tailnet(&self) -> bool {
        matches!(self, Self::Tailnet)
    }

    pub fn is_wan(&self) -> bool {
        matches!(self, Self::Wan)
    }

    /// Whether this network is considered locally trusted (e.g. for admin access when admin_lan_only is active).
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Lan | Self::Tailnet)
    }
}

/// Classify an IP address into LAN, Tailnet, or WAN.
pub fn classify_ip(ip: IpAddr) -> NetworkClass {
    match ip {
        IpAddr::V4(v4) => classify_ipv4(v4),
        IpAddr::V6(v6) => classify_ipv6(v6),
    }
}

fn classify_ipv4(ip: Ipv4Addr) -> NetworkClass {
    let octets = ip.octets();

    // Loopback 127.0.0.0/8
    if ip.is_loopback() {
        return NetworkClass::Lan;
    }

    // Link-local 169.254.0.0/16
    if ip.is_link_local() {
        return NetworkClass::Lan;
    }

    // RFC 1918 Private ranges:
    // 10.0.0.0/8
    if octets[0] == 10 {
        return NetworkClass::Lan;
    }
    // 172.16.0.0/12
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return NetworkClass::Lan;
    }
    // 192.168.0.0/16
    if octets[0] == 192 && octets[1] == 168 {
        return NetworkClass::Lan;
    }

    // Tailscale IPv4 CGNAT range: 100.64.0.0/10 (100.64.0.0 to 100.127.255.255)
    if octets[0] == 100 && (64..=127).contains(&octets[1]) {
        return NetworkClass::Tailnet;
    }

    NetworkClass::Wan
}

fn classify_ipv6(ip: Ipv6Addr) -> NetworkClass {
    let octets = ip.octets();

    // Loopback ::1
    if ip.is_loopback() {
        return NetworkClass::Lan;
    }

    // Link-local fe80::/10 (first byte 0xfe, top 2 bits of second byte are 10 -> 0x80..0xbf)
    if (octets[0] == 0xfe) && ((octets[1] & 0xc0) == 0x80) {
        return NetworkClass::Lan;
    }

    // Tailscale IPv6 prefix: fd7a:115c:a1e0::/48
    if octets[0] == 0xfd
        && octets[1] == 0x7a
        && octets[2] == 0x11
        && octets[3] == 0x5c
        && octets[4] == 0xa1
        && octets[5] == 0xe0
    {
        return NetworkClass::Tailnet;
    }

    // IPv6 Unique Local Address fc00::/7 (fc00.. to fdff..)
    if (octets[0] & 0xfe) == 0xfc {
        return NetworkClass::Lan;
    }

    NetworkClass::Wan
}

/// Resolve the effective client IP from peer socket address and optional forward headers,
/// strictly enforcing that proxy headers are ignored unless peer_ip is in `trusted_proxies`.
pub fn resolve_client_ip(
    peer_addr: SocketAddr,
    forwarded_for: Option<&str>,
    trusted_proxies: &[IpAddr],
) -> IpAddr {
    let peer_ip = peer_addr.ip();

    // If the direct peer is not in trusted_proxies, ignore any forwarded headers.
    let is_trusted_proxy = trusted_proxies.contains(&peer_ip) || peer_ip.is_loopback();
    if !is_trusted_proxy {
        return peer_ip;
    }

    let Some(header) = forwarded_for else {
        return peer_ip;
    };

    // Parse X-Forwarded-For: client, proxy1, proxy2
    // Take the leftmost IP that parses validly
    for part in header.split(',') {
        let clean = part.trim();
        if let Ok(ip) = clean.parse::<IpAddr>() {
            return ip;
        }
    }

    peer_ip
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_classification() {
        // LAN IPv4
        assert_eq!(classify_ip("127.0.0.1".parse().unwrap()), NetworkClass::Lan);
        assert_eq!(classify_ip("10.0.0.15".parse().unwrap()), NetworkClass::Lan);
        assert_eq!(classify_ip("172.20.5.1".parse().unwrap()), NetworkClass::Lan);
        assert_eq!(classify_ip("192.168.1.50".parse().unwrap()), NetworkClass::Lan);
        assert_eq!(classify_ip("169.254.1.1".parse().unwrap()), NetworkClass::Lan);

        // LAN IPv6
        assert_eq!(classify_ip("::1".parse().unwrap()), NetworkClass::Lan);
        assert_eq!(classify_ip("fe80::1ff:fe23:4567:890a".parse().unwrap()), NetworkClass::Lan);
        assert_eq!(classify_ip("fd00::1".parse().unwrap()), NetworkClass::Lan);

        // Tailnet
        assert_eq!(classify_ip("100.64.0.1".parse().unwrap()), NetworkClass::Tailnet);
        assert_eq!(classify_ip("100.100.50.25".parse().unwrap()), NetworkClass::Tailnet);
        assert_eq!(classify_ip("100.127.255.254".parse().unwrap()), NetworkClass::Tailnet);
        assert_eq!(classify_ip("fd7a:115c:a1e0::1".parse().unwrap()), NetworkClass::Tailnet);
        assert_eq!(classify_ip("fd7a:115c:a1e0:ab12::34".parse().unwrap()), NetworkClass::Tailnet);

        // WAN
        assert_eq!(classify_ip("8.8.8.8".parse().unwrap()), NetworkClass::Wan);
        assert_eq!(classify_ip("1.1.1.1".parse().unwrap()), NetworkClass::Wan);
        assert_eq!(classify_ip("100.128.0.1".parse().unwrap()), NetworkClass::Wan); // Beyond /10
        assert_eq!(classify_ip("2606:4700:4700::1111".parse().unwrap()), NetworkClass::Wan);
    }

    #[test]
    fn test_proxy_trust_resolution() {
        let untrusted_peer: SocketAddr = "192.168.1.100:54321".parse().unwrap();
        let loopback_peer: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let trusted_proxy: SocketAddr = "10.0.0.2:8080".parse().unwrap();
        let trusted_list = vec!["10.0.0.2".parse().unwrap()];

        // Untrusted peer attempting to spoof X-Forwarded-For
        let ip1 = resolve_client_ip(untrusted_peer, Some("1.2.3.4"), &trusted_list);
        assert_eq!(ip1, "192.168.1.100".parse::<IpAddr>().unwrap());

        // Loopback peer forwarding valid client
        let ip2 = resolve_client_ip(loopback_peer, Some("100.64.5.10, 127.0.0.1"), &trusted_list);
        assert_eq!(ip2, "100.64.5.10".parse::<IpAddr>().unwrap());

        // Configured trusted proxy forwarding WAN client
        let ip3 = resolve_client_ip(trusted_proxy, Some("8.8.8.8"), &trusted_list);
        assert_eq!(ip3, "8.8.8.8".parse::<IpAddr>().unwrap());
    }
}
