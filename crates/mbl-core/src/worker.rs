//! Typed Worker and Remote Node structures (M8.1).
//! Enforces typed job specifications without raw shell commands.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::WorkerId;

/// One working encoder on a worker, proven by the worker's own FFmpeg self-test.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerEncoder {
    /// Transcode backend name (`nvenc`, `qsv`, `vaapi`, `d3d11Amf`, `software`).
    pub backend: String,
    /// Video codec (`h264`, `hevc`, `av1`).
    pub codec: String,
}

/// Long-poll request: the worker reports what it can run right now and waits for work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPollRequest {
    pub encoders: Vec<WorkerEncoder>,
    pub max_jobs: u32,
    /// True while the host is busy with something else (for example a game) and must not get new jobs.
    pub paused: bool,
    /// Jobs the worker is still running; anything else the server believes is running has failed.
    pub active_job_ids: Vec<String>,
}

/// A transcode assignment. The server never sends command lines: `pipeline` is the server's
/// typed transcode model (`mbl_transcode::Pipeline`), which the worker turns into FFmpeg arguments
/// itself after replacing the input with `sourcePath` and the output with a local directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerJob {
    pub job_id: String,
    /// Server-relative signed source URL (`/api/v1/stream/worker/source/{partId}?token=…`).
    pub source_path: String,
    /// Rendition directory names; uploads go to `/api/v1/workers/jobs/{jobId}/files/{rendition}/{file}`.
    pub renditions: Vec<String>,
    #[schema(value_type = Object)]
    pub pipeline: serde_json::Value,
}

/// Instruction delivered to a worker in a poll response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkerMessage {
    Start {
        job: WorkerJob,
    },
    #[serde(rename_all = "camelCase")]
    Stop {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Pause {
        job_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Resume {
        job_id: String,
    },
}

/// Poll response; an empty list means the wait timed out and the worker should poll again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPollResponse {
    pub messages: Vec<WorkerMessage>,
}

/// Final outcome of a job reported by the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum WorkerJobOutcome {
    Finished,
    Failed,
}

/// Job outcome report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerJobReport {
    pub outcome: WorkerJobOutcome,
    /// Short reason for a failure (FFmpeg's last error lines), never shown to end users verbatim.
    pub message: Option<String>,
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
