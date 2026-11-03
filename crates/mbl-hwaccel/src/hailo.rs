//! Hailo NPU media & AI acceleration and cooperative co-tenant resource sharing (M4.17, ADR 0006).
//!
//! Supports Hailo-8L (13 TOPS), Hailo-8 (26 TOPS), and Hailo-10H (40 TOPS GenAI) NPUs.
//! Clarifies that Hailo is a neural network inference accelerator (not a traditional fixed-function
//! H.264/HEVC video bitstream transcoder) and coordinates shared `VDevice` access (`group_id = "SHARED"`)
//! plus duty-cycle pacing so co-tenants (e.g. Frigate, `hailo-ollama`, `rpicam-apps`, Home Assistant)
//! are never locked out or starved.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    /// Hardware-accelerated Whisper speech-to-text subtitle generation (`hailo-whisper` / `Speech2Text`).
    WhisperAsr,
    /// AI Video Super Resolution / Upscaling (SD/720p anime & older series -> 1080p via `.hef` CNN).
    VideoSuperResolution,
    /// Visual scene classification & OCR for intro/credits detection without heavy CPU decode.
    SceneIntroDetection,
    /// CLIP / SigLIP / Qwen-VL-vision visual & facial embeddings for photo and scene search.
    ClipSemanticIndexing,
    /// On-device GenAI LLM execution (Llama 3.2 / Qwen 2.5 / Qwen 3 / Gemma 4 / DeepSeek on Hailo-10H 40 TOPS).
    GenAiLlm,
    /// On-device GenAI Vision-Language Model (`hailort::genai::VLM`: Qwen2-VL-2B, Qwen3-VL-2B, Qwen3-VL-4B on Hailo-10H).
    GenAiVlm,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nnc_utilization_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ram_used_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ram_total_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_die_voltage_mv: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_hef_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_vlm_hef_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_llm_hef_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_whisper_hef_model: Option<String>,
}

/// Live hardware telemetry snapshot queried from `hailortcli` / `libhailort`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HailoLiveTelemetry {
    pub firmware_version: Option<String>,
    pub temperature_c: Option<f64>,
    pub nnc_utilization_percent: Option<f64>,
    pub ram_used_mb: Option<u64>,
    pub ram_total_mb: Option<u64>,
    pub on_die_voltage_mv: Option<u32>,
    pub active_hef_model: Option<String>,
    pub active_vlm_hef_model: Option<String>,
    pub active_llm_hef_model: Option<String>,
    pub active_whisper_hef_model: Option<String>,
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
                HailoPipelineCapability::GenAiVlm,
            ],
        }
    }

    /// Build a [`HailoRuntimeStatus`] snapshot for the given hardware fingerprint.
    pub fn status(
        &self,
        fp: &HardwareFingerprint,
        device_busy_override: Option<bool>,
    ) -> HailoRuntimeStatus {
        self.status_from_root(Path::new("/"), fp, device_busy_override)
    }

    /// Build a [`HailoRuntimeStatus`] snapshot against a specified filesystem root.
    pub fn status_from_root(
        &self,
        root: &Path,
        fp: &HardwareFingerprint,
        device_busy_override: Option<bool>,
    ) -> HailoRuntimeStatus {
        let co_tenants = detect_hailo_co_tenants(root);
        let busy = device_busy_override.unwrap_or(false);
        let supported_pipelines = if self.config.mode == HailoSharingMode::Disabled {
            Vec::new()
        } else {
            Self::pipelines_for_profile(fp.npu_profile)
        };

        let telemetry = if fp.npu_profile.is_present() && root == Path::new("/") {
            probe_hailo_live_telemetry(root, fp)
        } else {
            HailoLiveTelemetry {
                firmware_version: fp.hailo_devices.first().and_then(|d| d.driver_version.clone()),
                active_hef_model: discover_hef_model_for_profile(root, fp.npu_profile)
                    .map(|p| p.to_string_lossy().into_owned()),
                active_vlm_hef_model: discover_vlm_hef_model(root, fp.npu_profile)
                    .map(|p| p.to_string_lossy().into_owned()),
                active_llm_hef_model: discover_llm_hef_model(root, fp.npu_profile)
                    .map(|p| p.to_string_lossy().into_owned()),
                active_whisper_hef_model: discover_whisper_hef_model(root, fp.npu_profile)
                    .map(|p| p.to_string_lossy().into_owned()),
                ..HailoLiveTelemetry::default()
            }
        };

        let external_inference_url = self.config.ollama_url.clone().or_else(|| {
            if co_tenants.iter().any(|c| c == "hailo-ollama")
                || root.join("usr/bin/hailo-ollama").exists()
            {
                Some("http://127.0.0.1:8000".to_owned())
            } else {
                None
            }
        });

        HailoRuntimeStatus {
            npu_profile: fp.npu_profile,
            sharing_mode: self.config.mode,
            vdevice_group_id: self.config.vdevice_group_id.clone(),
            duty_cycle_percent: self.config.duty_cycle_percent,
            device_busy_or_locked: busy,
            active_co_tenants: co_tenants,
            external_inference_url,
            supported_pipelines,
            firmware_version: telemetry.firmware_version,
            temperature_c: telemetry.temperature_c,
            nnc_utilization_percent: telemetry.nnc_utilization_percent,
            ram_used_mb: telemetry.ram_used_mb,
            ram_total_mb: telemetry.ram_total_mb,
            on_die_voltage_mv: telemetry.on_die_voltage_mv,
            active_hef_model: telemetry.active_hef_model,
            active_vlm_hef_model: telemetry.active_vlm_hef_model,
            active_llm_hef_model: telemetry.active_llm_hef_model,
            active_whisper_hef_model: telemetry.active_whisper_hef_model,
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

/// Best-effort scan of `/proc` comm and cmdline for common Hailo NPU co-tenant processes/services
/// (e.g., `hailort_service`, `hailo-ollama`, `frigate` / `frigate.detector:hailo*`, `rpicam-vid`)
/// for diagnostic display.
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
        let pid_dir = entry.path();
        if let Ok(comm) = std::fs::read_to_string(pid_dir.join("comm")) {
            let trimmed = comm.trim();
            if matches!(
                trimmed,
                "hailort_service"
                    | "hailo-ollama"
                    | "frigate"
                    | "rpicam-vid"
                    | "rpicam-hello"
                    | "wyoming-hailo"
                    | "scrypted"
            ) {
                found.insert(trimmed.to_owned());
                continue;
            }
            // Linux kernel truncates `comm` to 15 chars (`frigate.detecto` for `frigate.detector:hailo10h`).
            if trimmed.starts_with("frigate.") {
                found.insert("frigate".to_owned());
                continue;
            }
        }
        if let Ok(cmdline_bytes) = std::fs::read(pid_dir.join("cmdline"))
            && !cmdline_bytes.is_empty()
        {
            let cmdline = String::from_utf8_lossy(&cmdline_bytes);
            if cmdline.contains("frigate.detector")
                || cmdline.contains("-m\0frigate")
                || cmdline.contains("-m frigate")
            {
                found.insert("frigate".to_owned());
            } else if cmdline.contains("hailo-ollama") {
                found.insert("hailo-ollama".to_owned());
            } else if cmdline.contains("hailort_service") {
                found.insert("hailort_service".to_owned());
            } else if cmdline.contains("wyoming-hailo") {
                found.insert("wyoming-hailo".to_owned());
            } else if cmdline.contains("gst-launch") && cmdline.contains("hailo") {
                found.insert("gstreamer-hailo".to_owned());
            }
        }
    }
    found.into_iter().collect()
}

/// User-configured `.hef` model path overrides managed at runtime by the server settings UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoModelOverrides {
    pub vision_hef_path: Option<String>,
    pub vlm_hef_path: Option<String>,
    pub llm_hef_path: Option<String>,
    pub whisper_hef_path: Option<String>,
}

static MODEL_OVERRIDES: OnceLock<Mutex<HailoModelOverrides>> = OnceLock::new();

fn overrides_mutex() -> &'static Mutex<HailoModelOverrides> {
    MODEL_OVERRIDES.get_or_init(|| Mutex::new(HailoModelOverrides::default()))
}

/// Update runtime `.hef` model path overrides and invalidate the 5-second telemetry cache.
pub fn set_hailo_model_overrides(overrides: HailoModelOverrides) {
    if let Ok(mut guard) = overrides_mutex().lock() {
        *guard = overrides;
    }
    invalidate_hailo_telemetry_cache();
}

/// Read current runtime `.hef` model path overrides.
pub fn get_hailo_model_overrides() -> HailoModelOverrides {
    overrides_mutex().lock().map(|g| g.clone()).unwrap_or_default()
}

/// Invalidate the 5-second in-memory telemetry cache so the next status request re-probes immediately.
pub fn invalidate_hailo_telemetry_cache() {
    if let Some(m) = TELEMETRY_CACHE.get()
        && let Ok(mut guard) = m.lock()
    {
        *guard = None;
    }
}

/// Discover a compatible `.hef` compiled neural network file on `root` for the given [`NpuProfile`].
///
/// Checks user-configured runtime overrides and `MBL_HAILO_HEF_PATH` first, then standard system & co-tenant model directories
/// (`/usr/share/hailo-models`, `/usr/local/share/hailo-models`, `/var/lib/mbl/models`,
/// `$HOME/frigate/config/model_cache`, `/home/marcus/frigate/config/model_cache`, `/config/model_cache`).
pub fn discover_hef_model_for_profile(root: &Path, profile: NpuProfile) -> Option<PathBuf> {
    if !profile.is_present() {
        return None;
    }

    if root == Path::new("/") {
        if let Some(custom) = get_hailo_model_overrides().vision_hef_path {
            let p = PathBuf::from(custom.trim());
            if p.is_file() && is_hef_compatible(&p, profile, true) {
                return Some(p);
            }
        }
        if let Ok(env_path) = std::env::var("MBL_HAILO_HEF_PATH") {
            let p = PathBuf::from(env_path.trim());
            if p.is_file() && is_hef_compatible(&p, profile, true) {
                return Some(p);
            }
        }
    }

    let mut search_dirs = vec![
        root.join("usr/share/hailo-models"),
        root.join("usr/local/share/hailo-models"),
        root.join("usr/share/hailo-ollama/models"),
        root.join("usr/share/hailo-ollama/models/blobs"),
        root.join("var/lib/hailo-ollama/models"),
        root.join("var/lib/mbl/models"),
        root.join("home/marcus/frigate/config/model_cache"),
        root.join("config/model_cache"),
        root.join("opt/frigate/model_cache"),
    ];

    if root == Path::new("/")
        && let Ok(home) = std::env::var("HOME")
    {
        let h = PathBuf::from(home);
        search_dirs.push(h.join(".hailo/models"));
        search_dirs.push(h.join(".local/share/hailo-ollama/models"));
        search_dirs.push(h.join(".local/share/hailo-ollama/models/blobs"));
        search_dirs.push(h.join("frigate/config/model_cache"));
        search_dirs.push(h.join("mbl/models"));
    }

    let preferred_names: &[&str] = match profile {
        NpuProfile::Hailo10H => &[
            "yolov11s.hef",
            "yolov8s.hef",
            "yolov8m.hef",
            "yolov11m.hef",
            "yolov8l.hef",
            "yolov11l.hef",
            "yolov8s_h10.hef",
            "Qwen3-VL-2B-vision.hef",
            "Qwen2-VL-2B-vision-336x336.hef",
            "Qwen2-VL-7B-vision-336x336.hef",
            "Qwen2-VL-7B-vision-252x448.hef",
            "clip_resnet_50x4.hef",
        ],
        NpuProfile::Hailo8 => &[
            "yolov8s_h8.hef",
            "yolov8m_h8.hef",
            "yolov8s.hef",
            "yolov8m.hef",
            "yolov6n.hef",
            "Qwen3-VL-2B-vision-hailo8.hef",
            "resnet_v1_50_h8.hef",
        ],
        NpuProfile::Hailo8L => &[
            "yolov8s_h8l.hef",
            "yolov8s_h8l_pi.hef",
            "yolov6n_h8l.hef",
            "yolov8s.hef",
            "yolov6n.hef",
            "resnet_v1_50_h8l.hef",
        ],
        NpuProfile::None => &[],
    };

    let mut candidates: Vec<PathBuf> = Vec::new();
    for dir in &search_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("hef")
                && p.is_file()
                && !candidates.contains(&p)
            {
                candidates.push(p);
            }
        }
    }

    // Sort candidates so preferred filenames come first
    candidates.sort_by_key(|p| {
        let fname = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
        preferred_names
            .iter()
            .position(|&pref| pref.eq_ignore_ascii_case(fname))
            .unwrap_or(usize::MAX)
    });

    let check_cli = root == Path::new("/") && Path::new("/usr/bin/hailortcli").exists();
    for candidate in &candidates {
        if is_genai_or_whisper_hef_filename(candidate) {
            continue;
        }
        if is_hef_compatible(candidate, profile, check_cli) {
            return Some(candidate.clone());
        }
    }

    None
}

fn is_genai_or_whisper_hef_filename(path: &Path) -> bool {
    let f = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if f.contains("-vision") {
        // Standalone vision encoders (e.g. Qwen3-VL-2B-vision.hef) ARE standard InferModel HEFs
        return false;
    }
    f.contains("instruct")
        || f.contains("deepseek")
        || f.contains("llama")
        || f.contains("gemma")
        || f.contains("whisper")
        || f.starts_with("qwen")
}

/// Discover a Hailo-10H Vision-Language Model (`hailort::genai::VLM`) `.hef` file on `root`.
///
/// Supports `Qwen3-VL-2B-Instruct.hef`, `Qwen2-VL-2B-Instruct.hef`, and `Qwen3-VL-4B-Instruct.hef`
/// (or custom path via `MBL_HAILO_VLM_HEF_PATH`).
pub fn discover_vlm_hef_model(root: &Path, profile: NpuProfile) -> Option<PathBuf> {
    if profile != NpuProfile::Hailo10H {
        return None;
    }
    if root == Path::new("/")
        && let Some(custom) = get_hailo_model_overrides().vlm_hef_path
    {
        let p = PathBuf::from(custom.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    discover_named_genai_hef(
        root,
        "MBL_HAILO_VLM_HEF_PATH",
        &["Qwen3-VL-2B-Instruct.hef", "Qwen2-VL-2B-Instruct.hef", "Qwen3-VL-4B-Instruct.hef"],
    )
}

/// Discover a Hailo-10H Large Language Model (`hailort::genai::LLM`) `.hef` file on `root`.
///
/// Supports all Hailo Model Zoo GenAI LLMs (`Qwen2.5-1.5B-Instruct.hef`, `Qwen3-1.7B-Instruct.hef`,
/// `Llama3.2-1B-Instruct.hef`, `Gemma4-E2B-Instruct.hef`, `DeepSeek-R1-Distill-Qwen-1.5B.hef`,
/// `Qwen3-4B-Instruct-2507.hef`, `Qwen2-1.5B-Instruct.hef`, `Qwen2-1.5B-Instruct-Function-Calling-v1.hef`,
/// `Qwen2.5-Coder-1.5B-Instruct.hef`, or custom path via `MBL_HAILO_LLM_HEF_PATH`).
pub fn discover_llm_hef_model(root: &Path, profile: NpuProfile) -> Option<PathBuf> {
    if profile != NpuProfile::Hailo10H {
        return None;
    }
    if root == Path::new("/")
        && let Some(custom) = get_hailo_model_overrides().llm_hef_path
    {
        let p = PathBuf::from(custom.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    discover_named_genai_hef(
        root,
        "MBL_HAILO_LLM_HEF_PATH",
        &[
            "Qwen2.5-1.5B-Instruct.hef",
            "Qwen3-1.7B-Instruct.hef",
            "Llama3.2-1B-Instruct.hef",
            "Llama-3.2-1B-Instruct.hef",
            "Gemma4-E2B-Instruct.hef",
            "DeepSeek-R1-Distill-Qwen-1.5B.hef",
            "Qwen2-1.5B-Instruct.hef",
            "Qwen3-4B-Instruct-2507.hef",
            "Qwen2-1.5B-Instruct-Function-Calling-v1.hef",
            "Qwen2.5-Coder-1.5B-Instruct.hef",
        ],
    )
}

/// Discover a Hailo-10H Whisper Speech-to-Text (`hailort::genai::Speech2Text`) `.hef` file on `root`.
///
/// Supports `Whisper-Base.hef`, `Whisper-Small.hef`, `Whisper-Tiny.hef`, and `Whisper-Medium.hef`
/// (or custom path via `MBL_HAILO_WHISPER_HEF_PATH`).
pub fn discover_whisper_hef_model(root: &Path, profile: NpuProfile) -> Option<PathBuf> {
    if profile != NpuProfile::Hailo10H {
        return None;
    }
    if root == Path::new("/")
        && let Some(custom) = get_hailo_model_overrides().whisper_hef_path
    {
        let p = PathBuf::from(custom.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    discover_named_genai_hef(
        root,
        "MBL_HAILO_WHISPER_HEF_PATH",
        &["Whisper-Base.hef", "Whisper-Small.hef", "Whisper-Tiny.hef", "Whisper-Medium.hef"],
    )
}

fn discover_named_genai_hef(
    root: &Path,
    env_var: &str,
    preferred_filenames: &[&str],
) -> Option<PathBuf> {
    if root == Path::new("/")
        && let Ok(env_path) = std::env::var(env_var)
    {
        let p = PathBuf::from(env_path.trim());
        if p.is_file() {
            return Some(p);
        }
    }

    let mut search_dirs = vec![
        root.join("usr/share/hailo-models"),
        root.join("usr/local/share/hailo-models"),
        root.join("usr/share/hailo-ollama/models"),
        root.join("usr/share/hailo-ollama/models/blobs"),
        root.join("var/lib/hailo-ollama/models"),
        root.join("var/lib/hailo-ollama/models/blobs"),
        root.join("var/lib/mbl/models"),
    ];
    if root == Path::new("/")
        && let Ok(home) = std::env::var("HOME")
    {
        let h = PathBuf::from(home);
        search_dirs.push(h.join(".hailo/models"));
        search_dirs.push(h.join(".local/share/hailo-ollama/models"));
        search_dirs.push(h.join(".local/share/hailo-ollama/models/blobs"));
        search_dirs.push(h.join("mbl/models"));
    }

    for &name in preferred_filenames {
        for dir in &search_dirs {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_hef_compatible(hef_path: &Path, profile: NpuProfile, use_cli: bool) -> bool {
    let fname = hef_path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_ascii_lowercase();

    // Quick filename rejection for mismatched architecture suffixes
    match profile {
        NpuProfile::Hailo10H => {
            if fname.contains("_h8l") || fname.contains("_h8.") {
                return false;
            }
        }
        NpuProfile::Hailo8 => {
            if fname.contains("_h8l") || fname.contains("_h10") {
                return false;
            }
        }
        NpuProfile::Hailo8L => {
            if fname.contains("_h10") || (fname.contains("_h8") && !fname.contains("_h8l")) {
                return false;
            }
        }
        NpuProfile::None => return false,
    }

    if use_cli
        && let Ok(out) = Command::new("/usr/bin/hailortcli")
            .args(["parse-hef", &hef_path.to_string_lossy()])
            .output()
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if let Some(compat) = line.strip_prefix("HEF Compatible for:") {
                let upper = compat.to_ascii_uppercase();
                return match profile {
                    NpuProfile::Hailo10H => upper.contains("HAILO10H") || upper.contains("HAILO15"),
                    NpuProfile::Hailo8 => upper.contains("HAILO8"),
                    NpuProfile::Hailo8L => upper.contains("HAILO8L") || upper.contains("HAILO8"),
                    NpuProfile::None => false,
                };
            }
        }
    }

    true
}

static TELEMETRY_CACHE: OnceLock<Mutex<Option<(Instant, HailoLiveTelemetry)>>> = OnceLock::new();

/// Query live Hailo NPU telemetry (`hailortcli fw-control identify` and `hailortcli monitor`)
/// with a 5-second in-memory cache so API requests remain fast.
pub fn probe_hailo_live_telemetry(root: &Path, fp: &HardwareFingerprint) -> HailoLiveTelemetry {
    let cache_mutex = TELEMETRY_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache_mutex.lock()
        && let Some((ts, cached)) = guard.as_ref()
        && ts.elapsed() < Duration::from_secs(5)
    {
        return cached.clone();
    }

    let mut telemetry = HailoLiveTelemetry {
        firmware_version: fp.hailo_devices.first().and_then(|d| d.driver_version.clone()),
        active_hef_model: discover_hef_model_for_profile(root, fp.npu_profile)
            .map(|p| p.to_string_lossy().into_owned()),
        active_vlm_hef_model: discover_vlm_hef_model(root, fp.npu_profile)
            .map(|p| p.to_string_lossy().into_owned()),
        active_llm_hef_model: discover_llm_hef_model(root, fp.npu_profile)
            .map(|p| p.to_string_lossy().into_owned()),
        active_whisper_hef_model: discover_whisper_hef_model(root, fp.npu_profile)
            .map(|p| p.to_string_lossy().into_owned()),
        ..HailoLiveTelemetry::default()
    };

    if root == Path::new("/") && Path::new("/usr/bin/hailortcli").exists() {
        // 1. Identify firmware version (~20ms)
        if let Ok(out) =
            Command::new("/usr/bin/hailortcli").args(["fw-control", "identify"]).output()
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(fw) = parse_hailortcli_identify_fw(&text) {
                telemetry.firmware_version = Some(fw);
            }
        }

        // 2. On Hailo-10H, read live NNC utilization, RAM usage, temperature, and voltage
        if fp.npu_profile == NpuProfile::Hailo10H
            && let Ok(out) =
                Command::new("timeout").args(["1.3", "/usr/bin/hailortcli", "monitor"]).output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            parse_hailortcli_monitor_h10(&text, &mut telemetry);
        }
    }

    if let Ok(mut guard) = cache_mutex.lock() {
        *guard = Some((Instant::now(), telemetry.clone()));
    }
    telemetry
}

/// Parse firmware version string from `hailortcli fw-control identify` output.
pub fn parse_hailortcli_identify_fw(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("Firmware Version:") {
            let ver = rest.split_whitespace().next().unwrap_or("").trim();
            if !ver.is_empty() {
                return Some(ver.to_owned());
            }
        }
    }
    None
}

/// Parse Hailo-10H performance & health stats table from `hailortcli monitor` output.
pub fn parse_hailortcli_monitor_h10(output: &str, telemetry: &mut HailoLiveTelemetry) {
    // Example line:
    // pci/0001:01:00.0      HAILO10H       20.8                  2.8                   4.2                   302 / 7221        51.0                     799
    for line in output.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("HAILO") {
            continue;
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        // tokens: [0]=device_id, [1]=arch, [2]=nnc_util, [3]=cpu_util, [4]=ram_util, [5]=ram_used, [6]="/", [7]=ram_total, [8]=temp_c, [9]=voltage_mv
        if tokens.len() >= 10 && tokens[6] == "/" {
            if let Ok(nnc) = tokens[2].parse::<f64>()
                && nnc >= 0.0
            {
                telemetry.nnc_utilization_percent = Some(nnc);
            }
            if let Ok(used) = tokens[5].parse::<u64>() {
                telemetry.ram_used_mb = Some(used);
            }
            if let Ok(total) = tokens[7].parse::<u64>() {
                telemetry.ram_total_mb = Some(total);
            }
            if let Ok(temp) = tokens[8].parse::<f64>()
                && temp > -40.0
            {
                telemetry.temperature_c = Some(temp);
            }
            if let Ok(mv) = tokens[9].parse::<u32>() {
                telemetry.on_die_voltage_mv = Some(mv);
            }
            break;
        }
    }
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

    #[test]
    fn detects_truncated_frigate_and_hailo_ollama_co_tenants() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("proc/5739")).unwrap();
        std::fs::write(root.join("proc/5739/comm"), "frigate.detecto\n").unwrap();
        std::fs::write(root.join("proc/5739/cmdline"), b"frigate.detector:hailo10h\0").unwrap();

        std::fs::create_dir_all(root.join("proc/3338")).unwrap();
        std::fs::write(root.join("proc/3338/comm"), "hailo-ollama\n").unwrap();

        let co_tenants = detect_hailo_co_tenants(root);
        assert_eq!(co_tenants, vec!["frigate".to_owned(), "hailo-ollama".to_owned()]);
    }

    #[test]
    fn parses_hailortcli_identify_and_h10_monitor_output() {
        let id_out = "Executing on device: pci/0001:01:00.0\nIdentifying board\nControl Protocol Version: 2\nFirmware Version: 5.4.0 (release,app)\nDevice Architecture: HAILO10H\n";
        assert_eq!(parse_hailortcli_identify_fw(id_out).as_deref(), Some("5.4.0"));

        let mon_out = "Device ID             Architecture   NNC Utilization (%)   CPU Utilization (%)   RAM Utilization (%)   RAM Usage (MB)    On Die Temperature (C)   On Die Voltage (mV)\n------------------------------------------------------------------------------------------------------------------------------------------------------------------------\npci/0001:01:00.0      HAILO10H       20.8                  2.8                   4.2                   302 / 7221        51.0                     799\n";
        let mut tel = HailoLiveTelemetry::default();
        parse_hailortcli_monitor_h10(mon_out, &mut tel);
        assert_eq!(tel.nnc_utilization_percent, Some(20.8));
        assert_eq!(tel.ram_used_mb, Some(302));
        assert_eq!(tel.ram_total_mb, Some(7221));
        assert_eq!(tel.temperature_c, Some(51.0));
        assert_eq!(tel.on_die_voltage_mv, Some(799));
    }

    #[test]
    fn discovers_architecture_appropriate_hef_models() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let models_dir = root.join("usr/share/hailo-models");
        std::fs::create_dir_all(&models_dir).unwrap();
        std::fs::write(models_dir.join("yolov8s_h8l.hef"), b"hef8l").unwrap();
        std::fs::write(models_dir.join("yolov8s_h8.hef"), b"hef8").unwrap();
        std::fs::write(models_dir.join("yolov11s.hef"), b"hef10").unwrap();

        let m8l = discover_hef_model_for_profile(root, NpuProfile::Hailo8L).unwrap();
        assert_eq!(m8l.file_name().unwrap().to_str(), Some("yolov8s_h8l.hef"));

        let m8 = discover_hef_model_for_profile(root, NpuProfile::Hailo8).unwrap();
        assert_eq!(m8.file_name().unwrap().to_str(), Some("yolov8s_h8.hef"));

        let m10 = discover_hef_model_for_profile(root, NpuProfile::Hailo10H).unwrap();
        assert_eq!(m10.file_name().unwrap().to_str(), Some("yolov11s.hef"));
    }

    #[test]
    fn discovers_hailo_genai_vlm_llm_whisper_and_qwen_vision_encoder_hefs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let models_dir = root.join("usr/share/hailo-models");
        std::fs::create_dir_all(&models_dir).unwrap();

        std::fs::write(models_dir.join("Qwen3-VL-2B-Instruct.hef"), b"vlm").unwrap();
        std::fs::write(models_dir.join("Qwen3-1.7B-Instruct.hef"), b"llm").unwrap();
        std::fs::write(models_dir.join("Whisper-Small.hef"), b"whisper").unwrap();
        std::fs::write(models_dir.join("Qwen3-VL-2B-vision.hef"), b"vision10").unwrap();
        std::fs::write(models_dir.join("Qwen3-VL-2B-vision-hailo8.hef"), b"vision8").unwrap();

        // 1. VLM, LLM, and Whisper discovery on Hailo-10H
        let vlm = discover_vlm_hef_model(root, NpuProfile::Hailo10H).unwrap();
        assert_eq!(vlm.file_name().unwrap().to_str(), Some("Qwen3-VL-2B-Instruct.hef"));
        assert!(discover_vlm_hef_model(root, NpuProfile::Hailo8).is_none());

        let llm = discover_llm_hef_model(root, NpuProfile::Hailo10H).unwrap();
        assert_eq!(llm.file_name().unwrap().to_str(), Some("Qwen3-1.7B-Instruct.hef"));
        assert!(discover_llm_hef_model(root, NpuProfile::Hailo8).is_none());

        let whisper = discover_whisper_hef_model(root, NpuProfile::Hailo10H).unwrap();
        assert_eq!(whisper.file_name().unwrap().to_str(), Some("Whisper-Small.hef"));
        assert!(discover_whisper_hef_model(root, NpuProfile::Hailo8L).is_none());

        // 2. Single-network `InferModel` discovery selects standalone Qwen3-VL vision encoder
        //    and ignores multi-stage LLM/VLM/Whisper `.hef` bundles
        let vis10 = discover_hef_model_for_profile(root, NpuProfile::Hailo10H).unwrap();
        assert_eq!(vis10.file_name().unwrap().to_str(), Some("Qwen3-VL-2B-vision.hef"));

        let vis8 = discover_hef_model_for_profile(root, NpuProfile::Hailo8).unwrap();
        assert_eq!(vis8.file_name().unwrap().to_str(), Some("Qwen3-VL-2B-vision-hailo8.hef"));
    }
}
