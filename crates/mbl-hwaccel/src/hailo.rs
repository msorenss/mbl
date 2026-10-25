//! Hailo NPU media & AI acceleration and cooperative co-tenant resource sharing (M4.17, ADR 0006).
//!
//! Supports Hailo-8L (13 TOPS), Hailo-8 (26 TOPS), and Hailo-10H (40 TOPS GenAI) NPUs.
//! Clarifies that Hailo is a neural network inference accelerator (not a traditional fixed-function
//! H.264/HEVC video bitstream transcoder) and coordinates shared `VDevice` access (`group_id = "SHARED"`)
//! plus duty-cycle pacing so co-tenants (e.g. Frigate, `hailo-ollama`, `rpicam-apps`, Home Assistant)
//! are never locked out or starved.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::fingerprint::{HardwareFingerprint, NpuProfile};

/// Default Hailo `VDevice` group ID shared across cooperative HailoRT processes.
pub const DEFAULT_VDEVICE_GROUP_ID: &str = "SHARED";

/// Cooperative NPU sharing mode (`[hw.hailo] mode` in config, ADR 0006).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HailoSharingMode {
    /// Cooperative sharing via `VDevice` (`group_id = "SHARED"`) + duty-cycle pacing + fallback if locked.
    #[default]
    Auto,
    /// Dedicated NPU mode (full throughput during idle windows when no co-tenants share the NPU).
    Exclusive,
    /// Never open `/dev/hailo*` or `/dev/h1x-*` directly; only use an external HTTP inference server (`ollama_url`).
    OffloadOnly,
    /// Do not use the Hailo NPU at all.
    Disabled,
}

impl HailoSharingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Exclusive => "exclusive",
            Self::OffloadOnly => "offload_only",
            Self::Disabled => "disabled",
        }
    }
}

/// AI media pipelines accelerated by Hailo-8 / Hailo-8L / Hailo-10H.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HailoPipelineCapability {
    /// Hardware-accelerated Whisper speech-to-text subtitle generation (`hailo-whisper`).
    WhisperAsr,
    /// AI Video Super Resolution / Upscaling (SD/720p anime & older series -> 1080p via `.hef` CNN).
    VideoSuperResolution,
    /// Visual scene classification & OCR for intro/credits detection without heavy CPU decode.
    SceneIntroDetection,
    /// CLIP / SigLIP visual & facial embeddings for photo and scene search.
    ClipSemanticIndexing,
    /// On-device GenAI LLM/VLM execution (Llama 3 / Qwen / Phi-3 / DeepSeek on Hailo-10H 40 TOPS).
    GenAiLlm,
}

/// Configuration for Hailo NPU resource sharing (`[hw.hailo]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoConfig {
    pub mode: HailoSharingMode,
    pub vdevice_group_id: String,
    /// Target maximum NPU duty cycle (1..=100) for background batch jobs in `Auto` mode.
    pub duty_cycle_percent: u8,
    /// Minimum pause between inference batches in `Auto` mode so real-time co-tenants get immediate scheduling slots.
    pub batch_sleep_ms: u64,
    /// Optional external Ollama / OpenAI-compatible inference endpoint (e.g. `http://127.0.0.1:8000` for `hailo-ollama`).
    pub ollama_url: Option<String>,
}

impl Default for HailoConfig {
    fn default() -> Self {
        Self {
            mode: HailoSharingMode::Auto,
            vdevice_group_id: DEFAULT_VDEVICE_GROUP_ID.to_owned(),
            duty_cycle_percent: 25,
            batch_sleep_ms: 50,
            ollama_url: None,
        }
    }
}

/// Live runtime status and co-tenant sharing state of the Hailo NPU.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoRuntimeStatus {
    pub npu_profile: NpuProfile,
    pub sharing_mode: HailoSharingMode,
    pub vdevice_group_id: String,
    pub duty_cycle_percent: u8,
    pub device_busy_or_locked: bool,
    pub active_co_tenants: Vec<String>,
    pub external_inference_url: Option<String>,
    pub supported_pipelines: Vec<HailoPipelineCapability>,
}

/// Cooperative duty-cycle pacer ensuring background NPU jobs yield to real-time co-tenants (ADR 0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DutyCyclePacer {
    pub mode: HailoSharingMode,
    pub duty_cycle_percent: u8,
    pub min_sleep_ms: u64,
}

impl DutyCyclePacer {
    pub fn new(config: &HailoConfig) -> Self {
        Self {
            mode: config.mode,
            duty_cycle_percent: config.duty_cycle_percent.clamp(5, 100),
            min_sleep_ms: config.batch_sleep_ms,
        }
    }

    /// Compute how long the worker should sleep after an inference batch of duration `inference_duration`
    /// to maintain the configured duty cycle and leave headroom for co-tenants.
    pub fn sleep_after_batch(&self, inference_duration: Duration) -> Duration {
        match self.mode {
            HailoSharingMode::Exclusive => Duration::ZERO,
            HailoSharingMode::OffloadOnly | HailoSharingMode::Disabled => {
                Duration::from_millis(self.min_sleep_ms)
            }
            HailoSharingMode::Auto => {
                let d = u64::from(self.duty_cycle_percent.clamp(5, 100));
                if d >= 100 {
                    return Duration::from_millis(self.min_sleep_ms);
                }
                let inf_ms = u64::try_from(inference_duration.as_millis()).unwrap_or(1).max(1);
                // duty_cycle = inf_ms / (inf_ms + sleep_ms)  =>  sleep_ms = inf_ms * (100 - d) / d
                let calculated_ms = (inf_ms.saturating_mul(100 - d)) / d;
                Duration::from_millis(calculated_ms.max(self.min_sleep_ms))
            }
        }
    }
}

/// Outcome when acquiring an NPU execution lease for a background AI task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "target", rename_all = "camelCase")]
pub enum NpuExecutionPlan {
    /// Execute on the local Hailo NPU using the shared `VDevice` group ID and duty-cycle pacing.
    HailoVDevice { device_node: String, vdevice_group_id: String, duty_cycle_percent: u8 },
    /// Offload inference to an external HTTP inference server (e.g. `hailo-ollama`) without opening `/dev/h1x-*` directly.
    ExternalHttp { url: String },
    /// Fall back to CPU or a remote `mbl-worker` because the NPU is disabled, absent, or locked exclusively by another tenant.
    CpuFallback { reason: String },
}

/// Coordinates NPU capability discovery, co-tenant detection, and fallback planning.
#[derive(Debug, Clone)]
pub struct NpuCoordinator {
    config: HailoConfig,
}

impl Default for NpuCoordinator {
    fn default() -> Self {
        Self::new(HailoConfig::default())
    }
}

impl NpuCoordinator {
    pub fn new(config: HailoConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &HailoConfig {
        &self.config
    }

    /// Return the supported AI pipelines for a given [`NpuProfile`].
    pub fn pipelines_for_profile(profile: NpuProfile) -> Vec<HailoPipelineCapability> {
        match profile {
            NpuProfile::None => Vec::new(),
            NpuProfile::Hailo8L | NpuProfile::Hailo8 => vec![
                HailoPipelineCapability::WhisperAsr,
                HailoPipelineCapability::VideoSuperResolution,
                HailoPipelineCapability::SceneIntroDetection,
                HailoPipelineCapability::ClipSemanticIndexing,
            ],
            NpuProfile::Hailo10H => vec![
                HailoPipelineCapability::WhisperAsr,
                HailoPipelineCapability::VideoSuperResolution,
                HailoPipelineCapability::SceneIntroDetection,
                HailoPipelineCapability::ClipSemanticIndexing,
                HailoPipelineCapability::GenAiLlm,
            ],
        }
    }

    /// Build a [`HailoRuntimeStatus`] snapshot for the given hardware fingerprint.
    pub fn status(
        &self,
        fp: &HardwareFingerprint,
        device_busy_override: Option<bool>,
    ) -> HailoRuntimeStatus {
        let co_tenants = detect_hailo_co_tenants(Path::new("/"));
        let busy = device_busy_override.unwrap_or(false);
        let supported_pipelines = if self.config.mode == HailoSharingMode::Disabled {
            Vec::new()
        } else {
            Self::pipelines_for_profile(fp.npu_profile)
        };

        HailoRuntimeStatus {
            npu_profile: fp.npu_profile,
            sharing_mode: self.config.mode,
            vdevice_group_id: self.config.vdevice_group_id.clone(),
            duty_cycle_percent: self.config.duty_cycle_percent,
            device_busy_or_locked: busy,
            active_co_tenants: co_tenants,
            external_inference_url: self.config.ollama_url.clone(),
            supported_pipelines,
        }
    }

    /// Plan how to execute an AI media task given the current hardware fingerprint and lock status.
    pub fn plan_execution(
        &self,
        fp: &HardwareFingerprint,
        capability: HailoPipelineCapability,
        device_exclusively_locked: bool,
    ) -> NpuExecutionPlan {
        if self.config.mode == HailoSharingMode::Disabled {
            return NpuExecutionPlan::CpuFallback {
                reason: "Hailo NPU acceleration is disabled by configuration".to_owned(),
            };
        }

        if self.config.mode == HailoSharingMode::OffloadOnly {
            if let Some(url) = &self.config.ollama_url {
                return NpuExecutionPlan::ExternalHttp { url: url.clone() };
            }
            return NpuExecutionPlan::CpuFallback {
                reason: "Hailo mode is offload_only and no external inference URL is configured"
                    .to_owned(),
            };
        }

        // For GenAI LLM tasks, prefer an already-running external inference endpoint (e.g. hailo-ollama)
        // to avoid loading duplicate LLM weights into NPU memory.
        if capability == HailoPipelineCapability::GenAiLlm
            && let Some(url) = &self.config.ollama_url
        {
            return NpuExecutionPlan::ExternalHttp { url: url.clone() };
        }

        if !fp.npu_profile.is_present() {
            return NpuExecutionPlan::CpuFallback {
                reason: "No Hailo NPU detected on host".to_owned(),
            };
        }

        if !Self::pipelines_for_profile(fp.npu_profile).contains(&capability) {
            return NpuExecutionPlan::CpuFallback {
                reason: format!(
                    "NPU profile '{}' does not support {:?}",
                    fp.npu_profile.as_str(),
                    capability
                ),
            };
        }

        if device_exclusively_locked {
            if let Some(url) = &self.config.ollama_url {
                return NpuExecutionPlan::ExternalHttp { url: url.clone() };
            }
            return NpuExecutionPlan::CpuFallback {
                reason: "Hailo NPU device is currently locked exclusively by a co-tenant process"
                    .to_owned(),
            };
        }

        let device_node = fp
            .hailo_devices
            .first()
            .map(|d| d.device_node.clone())
            .unwrap_or_else(|| "/dev/hailo0".to_owned());

        NpuExecutionPlan::HailoVDevice {
            device_node,
            vdevice_group_id: self.config.vdevice_group_id.clone(),
            duty_cycle_percent: if self.config.mode == HailoSharingMode::Exclusive {
                100
            } else {
                self.config.duty_cycle_percent
            },
        }
    }
}

/// Best-effort scan of `/proc` comm names for common Hailo NPU co-tenant processes/services
/// (e.g., `hailort_service`, `hailo-ollama`, `frigate`, `rpicam-vid`) for diagnostic display.
pub fn detect_hailo_co_tenants(root: &Path) -> Vec<String> {
    let mut found = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(root.join("proc")) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
            let trimmed = comm.trim();
            if matches!(
                trimmed,
                "hailort_service"
                    | "hailo-ollama"
                    | "frigate"
                    | "rpicam-vid"
                    | "rpicam-hello"
                    | "wyoming-hailo"
            ) {
                found.insert(trimmed.to_owned());
            }
        }
    }
    found.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::fingerprint::detect_fingerprint_from_root;

    #[test]
    fn duty_cycle_pacer_calculates_cooperative_sleep() {
        let cfg = HailoConfig {
            mode: HailoSharingMode::Auto,
            vdevice_group_id: "SHARED".to_owned(),
            duty_cycle_percent: 25,
            batch_sleep_ms: 50,
            ollama_url: None,
        };
        let pacer = DutyCyclePacer::new(&cfg);
        // 100ms inference at 25% duty cycle => 300ms sleep
        assert_eq!(pacer.sleep_after_batch(Duration::from_millis(100)), Duration::from_millis(300));
        // 5ms inference at 25% duty cycle => 15ms, clamped to min_sleep_ms (50ms)
        assert_eq!(pacer.sleep_after_batch(Duration::from_millis(5)), Duration::from_millis(50));

        let excl = DutyCyclePacer::new(&HailoConfig { mode: HailoSharingMode::Exclusive, ..cfg });
        assert_eq!(excl.sleep_after_batch(Duration::from_millis(100)), Duration::ZERO);
    }

    #[test]
    fn coordinator_falls_back_gracefully_when_device_is_locked() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("dev")).unwrap();
        std::fs::write(root.join("dev/h1x-0"), "").unwrap();

        let fp = detect_fingerprint_from_root(root, "aarch64", None);
        assert_eq!(fp.npu_profile, NpuProfile::Hailo10H);

        let coord = NpuCoordinator::default();
        // Unlocked -> uses shared VDevice with group_id = "SHARED"
        let plan_ok = coord.plan_execution(&fp, HailoPipelineCapability::WhisperAsr, false);
        assert_eq!(
            plan_ok,
            NpuExecutionPlan::HailoVDevice {
                device_node: "/dev/h1x-0".to_owned(),
                vdevice_group_id: "SHARED".to_owned(),
                duty_cycle_percent: 25,
            }
        );

        // Exclusively locked by another process -> falls back to CPU without erroring
        let plan_busy = coord.plan_execution(&fp, HailoPipelineCapability::WhisperAsr, true);
        assert!(matches!(plan_busy, NpuExecutionPlan::CpuFallback { .. }));

        // GenAI LLM on Hailo-8L -> falls back because only Hailo-10H supports GenAI LLMs
        let mut fp_8l = fp.clone();
        fp_8l.npu_profile = NpuProfile::Hailo8L;
        let plan_8l_llm = coord.plan_execution(&fp_8l, HailoPipelineCapability::GenAiLlm, false);
        assert!(matches!(plan_8l_llm, NpuExecutionPlan::CpuFallback { .. }));
    }
}
