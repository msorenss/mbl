//! `mbl-remote`: Remote access, Tailscale LocalAPI client, network classification,
//! bandwidth fair-share allocation, CGNAT reachability, and ACME/UPnP configurations (Milestone M6).

pub mod bandwidth;
pub mod identity;
pub mod network;
pub mod reachability;
pub mod tailscale;

pub use bandwidth::{BandwidthManager, NetworkQualityPreset};
pub use identity::{
    TailscaleAuthIdentity, evaluate_tailscale_serve_headers, resolve_tailnet_peer_identity,
};
pub use network::{NetworkClass, classify_ip, resolve_client_ip};
pub use reachability::{ReachabilityReport, RemoteRecommendation, analyze_reachability};
pub use tailscale::{
    TailscaleClient, TailscalePeer, TailscaleStatus, TailscaleTransport, TailscaleWhois,
};
