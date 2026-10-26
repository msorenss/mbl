//! Typed Worker and Remote Node structures (M8.1).
//! Enforces typed job specifications without raw shell commands.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{PartId, WorkerId};

/// Typed job specification dispatched to a transcode worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerJobSpec {
    pub session_id: String,
    pub part_id: PartId,
    pub source_url: String,
    pub target_format: String,
    pub segment_duration_secs: u32,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_bitrate_kbps: Option<u32>,
    pub audio_bitrate_kbps: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub audio_channels: Option<u32>,
    pub start_time_offset_secs: Option<f64>,
    pub burn_subtitle_stream: Option<u32>,
    pub upload_url_template: String,
}

/// Worker hardware capabilities reported during pairing and health checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkerCapabilities {
    pub gpu_vendor: String,
    pub gpu_device_name: String,
    pub supported_video_decoders: Vec<String>,
    pub supported_video_encoders: Vec<String>,
    pub supports_hardware_tonemap: bool,
    pub max_concurrent_sessions: u32,
}

/// Worker status and live telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStatus {
    pub state: String, // "idle", "busy", "paused_gaming", "offline"
    pub active_sessions: u32,
    pub cpu_usage_percent: f32,
    pub gpu_usage_percent: Option<f32>,
}

/// Dynamic control actions sent to an active worker session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "action", content = "params", rename_all = "camelCase")]
pub enum WorkerControl {
    Pause,
    Resume,
    Cancel,
    Seek { segment_number: u64 },
}

/// Heartbeat message sent periodically by workers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerHeartbeat {
    pub worker_id: WorkerId,
    pub status: WorkerStatus,
    pub active_job_ids: Vec<String>,
}

/// Worker pairing request payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPairRequest {
    pub pairing_code: String,
    pub name: String,
    pub hostname: Option<String>,
    pub mac_address: Option<String>,
    pub capabilities: WorkerCapabilities,
    pub client_cert_fingerprint: Option<String>,
}

/// Worker pairing response payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPairResponse {
    pub worker_id: WorkerId,
    pub auth_token: String,
    pub server_version: String,
    pub server_name: String,
}

/// Path mapping allowing workers to read shared network media directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPathMapping {
    pub server_prefix: String,
    pub worker_prefix: String,
}

impl WorkerPathMapping {
    /// Translates a server file path to a local worker file path if matching prefix.
    pub fn translate(&self, server_path: &str) -> Option<String> {
        if let Some(rest) = server_path.strip_prefix(&self.server_prefix) {
            let clean_rest = rest.trim_start_matches(['/', '\\']);
            let base = self.worker_prefix.trim_end_matches(['/', '\\']);
            let sep = if self.worker_prefix.contains('\\') || self.worker_prefix.contains(':') {
                '\\'
            } else {
                '/'
            };
            Some(format!("{base}{sep}{clean_rest}"))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_mapping_translation() {
        let mapping_win = WorkerPathMapping {
            server_prefix: "/mnt/nas/movies".into(),
            worker_prefix: r"Z:\movies".into(),
        };
        assert_eq!(
            mapping_win.translate("/mnt/nas/movies/SciFi/Inception.mkv"),
            Some(r"Z:\movies\SciFi/Inception.mkv".into())
        );

        let mapping_linux = WorkerPathMapping {
            server_prefix: "/mnt/nas/movies".into(),
            worker_prefix: "/data/movies".into(),
        };
        assert_eq!(
            mapping_linux.translate("/mnt/nas/movies/SciFi/Inception.mkv"),
            Some("/data/movies/SciFi/Inception.mkv".into())
        );

        assert_eq!(mapping_linux.translate("/var/media/other.mkv"), None);
    }
}
