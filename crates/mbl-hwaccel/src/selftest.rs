//! Functional hardware self-test (M4.3), fallback chains with TTL bad-backend tracking (M4.9),
//! and live transcode capacity modeling (M4.11).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use mbl_core::time::now_ms;
use mbl_platform::PlatformProfile;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ffmpeg::FfmpegProbeCaps;
use crate::fingerprint::HardwareFingerprint;

/// Default TTL (10 minutes) for a `(backend, codec)` marked bad after an early FFmpeg failure (M4.9).
pub const DEFAULT_BAD_BACKEND_TTL_MS: i64 = 10 * 60 * 1_000;

/// Minimum FPS required per concurrent 1080p stream when deriving `max_sessions` from self-test FPS (M4.11).
pub const FPS_PER_STREAM_BUDGET: f64 = 30.0;

/// One row in `hw_capabilities` recording the result of a functional decode/encode self-test (M4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HwCapEntry {
    pub backend: String,
    pub codec: String,
    pub operation: String,
    pub ok: bool,
    pub fps: Option<f64>,
    pub error: Option<String>,
    pub ffmpeg_hash: String,
    pub driver_fingerprint: String,
    pub tested_at: i64,
}

/// Record of a runtime transcode backend failure tracked with a TTL (M4.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BadBackendFailure {
    pub backend: String,
    pub codec: String,
    pub reason: String,
    pub failed_at: i64,
    pub expires_at: i64,
}

/// Thread-safe tracker that marks `(backend, codec)` pairs bad with a TTL when FFmpeg exits
/// non-zero within 5 seconds (M4.9).
#[derive(Debug, Clone)]
pub struct BadBackendTracker {
    ttl_ms: i64,
    entries: Arc<RwLock<HashMap<(String, String), BadBackendFailure>>>,
}

impl Default for BadBackendTracker {
    fn default() -> Self {
        Self::new(DEFAULT_BAD_BACKEND_TTL_MS)
    }
}

impl BadBackendTracker {
    pub fn new(ttl_ms: i64) -> Self {
        Self { ttl_ms: ttl_ms.max(1_000), entries: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Record a failure for `(backend, codec)` at the current timestamp.
    pub fn mark_bad(&self, backend: &str, codec: &str, reason: impl Into<String>) {
        self.mark_bad_at(backend, codec, reason, now_ms());
    }

    pub fn mark_bad_at(&self, backend: &str, codec: &str, reason: impl Into<String>, now: i64) {
        let key = (backend.to_owned(), codec.to_ascii_lowercase());
        let failure = BadBackendFailure {
            backend: backend.to_owned(),
            codec: codec.to_ascii_lowercase(),
            reason: reason.into(),
            failed_at: now,
            expires_at: now.saturating_add(self.ttl_ms),
        };
        if let Ok(mut map) = self.entries.write() {
            map.insert(key, failure);
        }
    }

    /// Check whether `(backend, codec)` is currently marked bad (not yet expired).
    pub fn is_bad(&self, backend: &str, codec: &str) -> bool {
        self.is_bad_at(backend, codec, now_ms())
    }

    pub fn is_bad_at(&self, backend: &str, codec: &str, now: i64) -> bool {
        let key = (backend.to_owned(), codec.to_ascii_lowercase());
        let Ok(map) = self.entries.read() else {
            return false;
        };
        map.get(&key).is_some_and(|f| f.expires_at > now)
    }

    /// List all non-expired backend failures.
    pub fn recent_failures(&self) -> Vec<BadBackendFailure> {
        let now = now_ms();
        let Ok(mut map) = self.entries.write() else {
            return Vec::new();
        };
        map.retain(|_, v| v.expires_at > now);
        let mut list: Vec<BadBackendFailure> = map.values().cloned().collect();
        list.sort_by_key(|a| std::cmp::Reverse(a.failed_at));
        list
    }

    pub fn clear(&self) {
        if let Ok(mut map) = self.entries.write() {
            map.clear();
        }
    }
}

/// Live transcode capacity summary derived from hardware profile, self-test FPS, and active sessions (M4.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranscodeCapacity {
    pub preferred_backend: String,
    pub fallback_backends: Vec<String>,
    pub encode_codecs: Vec<String>,
    pub can_tonemap: bool,
    pub supports_abr_ladder: bool,
    pub max_video_sessions: usize,
    pub active_video_sessions: usize,
    pub available_video_sessions: usize,
    pub can_transcode_video: bool,
}

/// Candidate hardware/software backends in priority order for a given [`PlatformProfile`] and [`FfmpegProbeCaps`].
pub fn candidate_backends_for_profile(
    profile: PlatformProfile,
    probe: &FfmpegProbeCaps,
) -> Vec<String> {
    let mut out = Vec::new();
    match profile {
        PlatformProfile::X86Nvidia => {
            if probe.has_encoder("h264_nvenc") {
                out.push("nvenc".to_owned());
            }
            if probe.has_encoder("h264_vaapi") {
                out.push("vaapi".to_owned());
            }
            if probe.has_encoder("h264_qsv") {
                out.push("qsv".to_owned());
            }
        }
        PlatformProfile::X86Intel => {
            if probe.has_encoder("h264_qsv") {
                out.push("qsv".to_owned());
            }
            if probe.has_encoder("h264_vaapi") {
                out.push("vaapi".to_owned());
            }
        }
        PlatformProfile::X86Amd => {
            if probe.has_encoder("h264_amf") {
                out.push("d3d11Amf".to_owned());
            }
            if probe.has_encoder("h264_vaapi") {
                out.push("vaapi".to_owned());
            }
        }
        PlatformProfile::Pi4 => {
            if probe.has_encoder("h264_v4l2m2m") {
                out.push("v4l2m2m".to_owned());
            }
        }
        PlatformProfile::Pi5 => {
            // M4.7: Pi 5 uses HEVC -hwaccel drm decode with software encode; never hardware H.264 encode.
            if probe.has_hwaccel("drm") {
                out.push("piDrmDecode".to_owned());
            }
        }
        PlatformProfile::GenericArm | PlatformProfile::Generic => {
            if probe.has_encoder("h264_nvenc") {
                out.push("nvenc".to_owned());
            }
            if probe.has_encoder("h264_qsv") {
                out.push("qsv".to_owned());
            }
            if probe.has_encoder("h264_vaapi") {
                out.push("vaapi".to_owned());
            }
            if probe.has_encoder("h264_amf") {
                out.push("d3d11Amf".to_owned());
            }
        }
    }
    out.push("software".to_owned());
    out
}

/// Compute the ordered fallback chain of backends for `codec` (M4.9), filtering out backends
/// that failed self-test or are currently marked bad in [`BadBackendTracker`].
pub fn fallback_chain(
    profile: PlatformProfile,
    probe: &FfmpegProbeCaps,
    results: &[HwCapEntry],
    tracker: &BadBackendTracker,
    codec: &str,
) -> Vec<String> {
    let candidates = candidate_backends_for_profile(profile, probe);
    let mut chain = Vec::new();

    for backend in candidates {
        // Never use v4l2m2m on Pi 5 (M4.7).
        if profile == PlatformProfile::Pi5 && backend == "v4l2m2m" {
            continue;
        }
        if tracker.is_bad(&backend, codec) {
            continue;
        }
        // Check if self-test explicitly recorded a failure for this backend + codec
        let explicit_fail = results.iter().any(|r| {
            r.backend == backend
                && r.operation == "encode"
                && r.codec.eq_ignore_ascii_case(codec)
                && !r.ok
        });
        if explicit_fail && backend != "software" {
            continue;
        }
        // Ensure backend actually supports the requested codec (or is software h264 fallback)
        let supported = probe.supported_encode_codecs_for_backend(&backend);
        if supported.iter().any(|c| c.eq_ignore_ascii_case(codec)) || backend == "software" {
            chain.push(backend);
        }
    }

    if chain.is_empty() {
        chain.push("software".to_owned());
    }
    chain
}

/// Compute [`TranscodeCapacity`] from hardware fingerprint, FFmpeg probe, self-test FPS rows,
/// bad-backend tracker, and active session count (M4.11).
pub fn compute_capacity(
    fp: &HardwareFingerprint,
    probe: &FfmpegProbeCaps,
    results: &[HwCapEntry],
    tracker: &BadBackendTracker,
    active_video_sessions: usize,
) -> TranscodeCapacity {
    let chain = fallback_chain(fp.profile, probe, results, tracker, "h264");
    let preferred_backend = chain.first().cloned().unwrap_or_else(|| "software".to_owned());
    let fallback_backends = chain.into_iter().skip(1).collect::<Vec<_>>();

    let mut encode_codecs = probe.supported_encode_codecs_for_backend(&preferred_backend);
    // Filter out codecs that failed self-test on the preferred backend
    encode_codecs.retain(|codec| {
        !tracker.is_bad(&preferred_backend, codec)
            && !results.iter().any(|r| {
                r.backend == preferred_backend
                    && r.operation == "encode"
                    && r.codec.eq_ignore_ascii_case(codec)
                    && !r.ok
            })
    });
    if encode_codecs.is_empty() {
        encode_codecs.push("h264".to_owned());
    }

    let can_tonemap = probe.can_tonemap_on_backend(&preferred_backend, fp.profile);
    let supports_abr_ladder = !fp.profile.is_pi_or_arm()
        && matches!(preferred_backend.as_str(), "vaapi" | "qsv" | "nvenc" | "d3d11Amf");

    let max_video_sessions = estimate_max_sessions(fp, &preferred_backend, results);
    let available_video_sessions = max_video_sessions.saturating_sub(active_video_sessions);
    let can_transcode_video = available_video_sessions > 0;

    TranscodeCapacity {
        preferred_backend,
        fallback_backends,
        encode_codecs,
        can_tonemap,
        supports_abr_ladder,
        max_video_sessions,
        active_video_sessions,
        available_video_sessions,
        can_transcode_video,
    }
}

fn estimate_max_sessions(
    fp: &HardwareFingerprint,
    preferred_backend: &str,
    results: &[HwCapEntry],
) -> usize {
    // Check if we have a measured FPS from self-test for the preferred backend
    let measured_fps = results
        .iter()
        .find(|r| {
            r.backend == preferred_backend && r.operation == "encode" && r.codec == "h264" && r.ok
        })
        .and_then(|r| r.fps);

    let raw_limit = if let Some(fps) = measured_fps {
        if fps < 24.0 { 0 } else { ((fps / FPS_PER_STREAM_BUDGET).floor() as usize).clamp(1, 16) }
    } else {
        match fp.profile {
            PlatformProfile::Pi4 | PlatformProfile::Pi5 | PlatformProfile::GenericArm => 1,
            PlatformProfile::Generic => 2,
            PlatformProfile::X86Intel | PlatformProfile::X86Nvidia | PlatformProfile::X86Amd => 4,
        }
    };

    // Cap by NVENC session limit when running on NVIDIA
    if preferred_backend == "nvenc"
        && let Some(gpu) = fp.nvidia_gpus.first()
    {
        return raw_limit.min(gpu.max_enc_sessions as usize);
    }

    // Cap Pi profiles at 1 concurrent video transcode to keep thermal & CPU headroom safe
    if fp.profile.is_pi_or_arm() {
        return raw_limit.min(1);
    }

    raw_limit
}

/// Run functional 2 s decode and 1080p encode self-tests across candidate backends (M4.3).
pub fn run_selftest(
    ffmpeg_bin: &Path,
    fp: &HardwareFingerprint,
    probe: &FfmpegProbeCaps,
) -> Vec<HwCapEntry> {
    let mut entries = Vec::new();
    let now = now_ms();
    let candidates = candidate_backends_for_profile(fp.profile, probe);

    for backend in candidates {
        // 1. Decode test (h264 + hevc)
        for decode_codec in ["h264", "hevc"] {
            let (ok, fps, error) = run_single_decode_test(ffmpeg_bin, &backend, decode_codec);
            entries.push(HwCapEntry {
                backend: backend.clone(),
                codec: decode_codec.to_owned(),
                operation: "decode".to_owned(),
                ok,
                fps,
                error,
                ffmpeg_hash: probe.binary_hash.clone(),
                driver_fingerprint: fp.driver_fingerprint.clone(),
                tested_at: now,
            });
        }

        // 2. Encode test (per supported codec on this backend)
        let mut encode_codecs = probe.supported_encode_codecs_for_backend(&backend);
        if encode_codecs.is_empty() && backend == "software" {
            encode_codecs.push("h264".to_owned());
        }
        for encode_codec in encode_codecs {
            let (ok, fps, error) = run_single_encode_test(ffmpeg_bin, &backend, &encode_codec);
            entries.push(HwCapEntry {
                backend: backend.clone(),
                codec: encode_codec,
                operation: "encode".to_owned(),
                ok,
                fps,
                error,
                ffmpeg_hash: probe.binary_hash.clone(),
                driver_fingerprint: fp.driver_fingerprint.clone(),
                tested_at: now,
            });
        }
    }

    entries
}

fn run_single_decode_test(
    ffmpeg_bin: &Path,
    backend: &str,
    _codec: &str,
) -> (bool, Option<f64>, Option<String>) {
    if !ffmpeg_bin.exists() && ffmpeg_bin != Path::new("ffmpeg") {
        return (false, None, Some("ffmpeg binary not found".to_owned()));
    }

    let mut cmd = Command::new(ffmpeg_bin);
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);
    match backend {
        "vaapi" => {
            cmd.args(["-hwaccel", "vaapi", "-hwaccel_device", "/dev/dri/renderD128"]);
        }
        "qsv" => {
            cmd.args(["-hwaccel", "qsv"]);
        }
        "nvenc" => {
            cmd.args(["-hwaccel", "cuda"]);
        }
        "piDrmDecode" => {
            cmd.args(["-hwaccel", "drm"]);
        }
        "d3d11Amf" => {
            cmd.args(["-hwaccel", "d3d11va"]);
        }
        _ => {}
    }

    let frames = 30u32;
    cmd.args([
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1280x720:rate=30",
        "-frames:v",
        &frames.to_string(),
        "-f",
        "null",
        "-",
    ]);

    execute_timed_ffmpeg(cmd, frames)
}

fn run_single_encode_test(
    ffmpeg_bin: &Path,
    backend: &str,
    codec: &str,
) -> (bool, Option<f64>, Option<String>) {
    if !ffmpeg_bin.exists() && ffmpeg_bin != Path::new("ffmpeg") {
        return (false, None, Some("ffmpeg binary not found".to_owned()));
    }

    let frames = 30u32;
    let mut cmd = Command::new(ffmpeg_bin);
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);

    match backend {
        "vaapi" => {
            cmd.args([
                "-vaapi_device",
                "/dev/dri/renderD128",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=30",
                "-frames:v",
                &frames.to_string(),
                "-vf",
                "format=nv12,hwupload",
                "-c:v",
                match codec {
                    "hevc" => "hevc_vaapi",
                    "av1" => "av1_vaapi",
                    _ => "h264_vaapi",
                },
                "-f",
                "null",
                "-",
            ]);
        }
        "qsv" => {
            cmd.args([
                "-init_hw_device",
                "qsv=hw",
                "-filter_hw_device",
                "hw",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=30",
                "-frames:v",
                &frames.to_string(),
                "-vf",
                "format=nv12,hwupload=extra_hw_frames=64,format=qsv",
                "-c:v",
                match codec {
                    "hevc" => "hevc_qsv",
                    "av1" => "av1_qsv",
                    _ => "h264_qsv",
                },
                "-f",
                "null",
                "-",
            ]);
        }
        "nvenc" => {
            cmd.args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=30",
                "-frames:v",
                &frames.to_string(),
                "-pix_fmt",
                "yuv420p",
                "-c:v",
                match codec {
                    "hevc" => "hevc_nvenc",
                    "av1" => "av1_nvenc",
                    _ => "h264_nvenc",
                },
                "-f",
                "null",
                "-",
            ]);
        }
        "v4l2m2m" => {
            cmd.args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=30",
                "-frames:v",
                &frames.to_string(),
                "-pix_fmt",
                "yuv420p",
                "-c:v",
                "h264_v4l2m2m",
                "-num_output_buffers",
                "32",
                "-num_capture_buffers",
                "16",
                "-f",
                "null",
                "-",
            ]);
        }
        "d3d11Amf" => {
            cmd.args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=30",
                "-frames:v",
                &frames.to_string(),
                "-pix_fmt",
                "yuv420p",
                "-c:v",
                match codec {
                    "hevc" => "hevc_amf",
                    "av1" => "av1_amf",
                    _ => "h264_amf",
                },
                "-f",
                "null",
                "-",
            ]);
        }
        _ => {
            // Software & piDrmDecode (software libx264/libx265/libsvtav1 encode)
            let enc = match codec {
                "hevc" => "libx265",
                "av1" => "libsvtav1",
                _ => "libx264",
            };
            cmd.args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1920x1080:rate=30",
                "-frames:v",
                &frames.to_string(),
                "-pix_fmt",
                "yuv420p",
                "-c:v",
                enc,
            ]);
            if enc != "libsvtav1" {
                cmd.args(["-preset", "ultrafast"]);
            }
            cmd.args(["-f", "null", "-"]);
        }
    }

    execute_timed_ffmpeg(cmd, frames)
}

fn execute_timed_ffmpeg(mut cmd: Command, frames: u32) -> (bool, Option<f64>, Option<String>) {
    let started = Instant::now();
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let elapsed = started.elapsed().as_secs_f64().max(0.005);
            let fps = ((f64::from(frames) / elapsed) * 10.0).round() / 10.0;
            (true, Some(fps), None)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            let msg = if stderr.is_empty() {
                format!("ffmpeg exited with status {}", out.status)
            } else {
                stderr.lines().last().unwrap_or(&stderr).to_owned()
            };
            (false, None, Some(msg))
        }
        Err(e) => (false, None, Some(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::ffmpeg::parse_from_outputs;
    use crate::fingerprint::detect_fingerprint_from_root;

    #[test]
    fn bad_backend_tracker_marks_and_expires_entries() {
        let tracker = BadBackendTracker::new(5_000);
        assert!(!tracker.is_bad_at("vaapi", "h264", 1_000));
        tracker.mark_bad_at("vaapi", "h264", "Driver error", 1_000);
        assert!(tracker.is_bad_at("vaapi", "h264", 3_000));
        assert!(!tracker.is_bad_at("vaapi", "h264", 7_000));
    }

    #[test]
    fn capacity_model_scales_sessions_from_selftest_fps_and_respects_pi_limits() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("sys/class/drm/card0/device")).unwrap();
        std::fs::write(root.join("sys/class/drm/card0/device/vendor"), "0x8086\n").unwrap();

        let fp = detect_fingerprint_from_root(root, "x86_64", None);
        let probe = parse_from_outputs(
            "hash1".to_owned(),
            "ffmpeg version 7.0",
            "Hardware acceleration methods:\nvaapi\nqsv\n",
            "------\n V....D h264_qsv test\n V....D hevc_qsv test\n V....D h264_vaapi test\n V....D libx264 test\n",
            "------\n",
            " ... vpp_qsv V->V test\n",
        );

        let results = vec![HwCapEntry {
            backend: "qsv".to_owned(),
            codec: "h264".to_owned(),
            operation: "encode".to_owned(),
            ok: true,
            fps: Some(155.0),
            error: None,
            ffmpeg_hash: "hash1".to_owned(),
            driver_fingerprint: fp.driver_fingerprint.clone(),
            tested_at: 100,
        }];

        let tracker = BadBackendTracker::default();
        let cap = compute_capacity(&fp, &probe, &results, &tracker, 2);
        assert_eq!(cap.preferred_backend, "qsv");
        assert_eq!(cap.fallback_backends, vec!["vaapi", "software"]);
        // 155 fps / 30 fps per stream = 5 max sessions
        assert_eq!(cap.max_video_sessions, 5);
        assert_eq!(cap.available_video_sessions, 3);
        assert!(cap.can_transcode_video);
        assert!(cap.can_tonemap);
        assert!(cap.supports_abr_ladder);

        // If qsv is marked bad, fallback chain switches to vaapi automatically
        tracker.mark_bad("qsv", "h264", "GPU hang");
        let cap_after_fail = compute_capacity(&fp, &probe, &results, &tracker, 0);
        assert_eq!(cap_after_fail.preferred_backend, "vaapi");
        assert_eq!(cap_after_fail.fallback_backends, vec!["software"]);
    }
}
