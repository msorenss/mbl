//! Hardware acceleration fingerprinting, FFmpeg capability probing, functional self-tests,
//! fallback chains, live capacity modeling, and cooperative Hailo NPU resource sharing (M4).

pub mod ffmpeg;
pub mod fingerprint;
pub mod hailo;
pub mod selftest;

pub use ffmpeg::{
    FfmpegProbeCaps, hash_ffmpeg_binary, parse_codecs, parse_filters, parse_from_outputs,
    parse_hwaccels, probe_ffmpeg,
};
pub use fingerprint::{
    DrmDevice, DxgiAdapter, HailoDeviceInfo, HardwareFingerprint, NpuProfile, NvidiaGpu,
    V4l2Device, detect_fingerprint, detect_fingerprint_from_root, parse_nvidia_smi_xml,
};
pub use hailo::{
    DEFAULT_VDEVICE_GROUP_ID, DutyCyclePacer, HailoConfig, HailoPipelineCapability,
    HailoRuntimeStatus, HailoSharingMode, NpuCoordinator, NpuExecutionPlan,
    detect_hailo_co_tenants,
};
pub use selftest::{
    BadBackendFailure, BadBackendTracker, DEFAULT_BAD_BACKEND_TTL_MS, FPS_PER_STREAM_BUDGET,
    HwCapEntry, TranscodeCapacity, candidate_backends_for_profile, compute_capacity,
    fallback_chain, run_selftest,
};
