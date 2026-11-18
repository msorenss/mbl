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
    /// Key the node uses to verify the server's signed file URLs. Absent from servers before 1.3.9.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_secret: Option<String>,
}

/// Single library summary exported by a storage node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeLibrary {
    pub remote_library_id: i64,
    pub name: String,
    /// `movie`, `show`, `music`, `photo` or `homevideo`.
    pub kind: String,
    pub item_count: i64,
    /// Complete file listing of the library. When present the server mirrors it as a regular
    /// MBL library; when absent only the summary is stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<NodeFileEntry>>,
}

/// One media file on a node, by path relative to its library root (`/` separated).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeFileEntry {
    pub path: String,
    pub size: i64,
    /// Modification time in Unix milliseconds.
    pub mtime: i64,
}

/// A file the server wants probe data for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeFileRef {
    pub remote_library_id: i64,
    pub path: String,
}

/// FFprobe result for one node file (`probe` is MBL's `MediaInfo` JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeProbedFile {
    pub remote_library_id: i64,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    #[schema(value_type = Object)]
    pub probe: serde_json::Value,
}

/// Probe upload for files the server asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeProbeUpload {
    pub files: Vec<NodeProbedFile>,
}

/// Result of a probe upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeProbeUploadResponse {
    pub imported: u32,
    pub failed: u32,
}

/// Scheme of part paths that live on a storage node: `mblnode://<nodeId>/<remoteLibraryId>/<path>`.
pub const NODE_URI_SCHEME: &str = "mblnode://";

/// Library root used for a node library.
pub fn node_library_root(node_id: NodeId, remote_library_id: i64) -> String {
    format!("{NODE_URI_SCHEME}{}/{remote_library_id}", node_id.0)
}

/// Split a node part path into `(node, library, relative path)`.
pub fn parse_node_path(path: &str) -> Option<(NodeId, i64, &str)> {
    let rest = path.strip_prefix(NODE_URI_SCHEME)?;
    let (node, rest) = rest.split_once('/')?;
    let (library, rel) = rest.split_once('/')?;
    let rel_ok = !rel.is_empty()
        && !rel.starts_with('/')
        && !rel.contains('\\')
        && !rel.contains('\0')
        && rel.split('/').all(|c| !c.is_empty() && c != "." && c != "..");
    rel_ok.then_some(())?;
    Some((NodeId(node.parse().ok()?), library.parse().ok()?, rel))
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
    /// New or changed files the node should probe and upload (bounded; sync again for more).
    #[serde(default)]
    pub need_probe: Vec<NodeFileRef>,
}

/// Smart handover response for direct playback or relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeHandoverResponse {
    pub direct_play_url: Option<String>,
    pub requires_relay: bool,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_paths_round_trip_and_reject_traversal() {
        let root = node_library_root(NodeId(3), 7);
        assert_eq!(root, "mblnode://3/7");
        assert_eq!(
            parse_node_path(&format!("{root}/Movies/Heat (1995).mkv")),
            Some((NodeId(3), 7, "Movies/Heat (1995).mkv"))
        );
        for bad in [
            "mblnode://3/7/../x.mkv",
            "mblnode://3/7//x.mkv",
            "mblnode://3/7/a\\b.mkv",
            "mblnode://x/7/a.mkv",
            "/srv/a.mkv",
            "mblnode://3/7/",
        ] {
            assert_eq!(parse_node_path(bad), None, "{bad}");
        }
    }
}
