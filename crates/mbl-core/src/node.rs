//! Typed Remote Node (Federated storage node) structures (M8.10, M8.11).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::NodeId;

/// Node capabilities reported during pairing and health checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodeCapabilities {
    pub gpu_vendor: Option<String>,
    pub supported_video_encoders: Vec<String>,
    pub supports_hardware_transcode: bool,
    pub storage_total_bytes: Option<u64>,
    pub storage_available_bytes: Option<u64>,
}

/// Remote library node pairing request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodePairRequest {
    pub pairing_code: String,
    pub name: String,
    pub base_url: String,
    pub capabilities: NodeCapabilities,
    pub client_cert_fingerprint: Option<String>,
}

/// Remote library node pairing response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodePairResponse {
    pub node_id: NodeId,
    pub auth_token: String,
    pub server_version: String,
    pub server_name: String,
}

/// Single library summary exported by a storage node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeLibrary {
    pub remote_library_id: i64,
    pub name: String,
    pub kind: String,
    pub item_count: i64,
}

/// Delta catalog synchronization request from a storage node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeCatalogSyncRequest {
    pub libraries: Vec<NodeLibrary>,
}

/// Catalog sync response returned by central server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeCatalogSyncResponse {
    pub synced_libraries: u32,
    pub total_items: i64,
}

/// Smart handover response for direct playback or relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeHandoverResponse {
    pub direct_play_url: Option<String>,
    pub requires_relay: bool,
    pub reason: String,
}
