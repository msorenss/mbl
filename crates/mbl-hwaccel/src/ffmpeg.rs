//! FFmpeg capability probe (`-hwaccels`, `-encoders`, `-decoders`, `-filters`) cached per binary hash (M4.2).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{LazyLock, RwLock};

use mbl_platform::PlatformProfile;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use xxhash_rust::xxh3::xxh3_64;

/// Discovered capabilities of an `ffmpeg` binary (M4.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegProbeCaps {
    pub binary_hash: String,
    pub version: String,
    pub hwaccels: Vec<String>,
    pub encoders: Vec<String>,
    pub decoders: Vec<String>,
    pub filters: Vec<String>,
    pub supports_tonemap_vaapi: bool,
    pub supports_tonemap_cuda: bool,
    pub supports_tonemap_opencl: bool,
    pub supports_libplacebo: bool,
    pub supports_vpp_qsv: bool,
    pub supports_zscale_tonemap: bool,
}

impl FfmpegProbeCaps {
    pub fn has_hwaccel(&self, name: &str) -> bool {
        self.hwaccels.iter().any(|h| h.eq_ignore_ascii_case(name))
    }

    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.iter().any(|e| e.eq_ignore_ascii_case(name))
    }

    pub fn has_decoder(&self, name: &str) -> bool {
        self.decoders.iter().any(|d| d.eq_ignore_ascii_case(name))
    }

    pub fn has_filter(&self, name: &str) -> bool {
        self.filters.iter().any(|f| f.eq_ignore_ascii_case(name))
    }

    /// Return the normalized target video codecs (`"h264"`, `"hevc"`, `"av1"`) supported by `backend`.
    pub fn supported_encode_codecs_for_backend(&self, backend: &str) -> Vec<String> {
        let mut out = Vec::new();
        match backend {
            "vaapi" => {
                if self.has_encoder("h264_vaapi") {
                    out.push("h264".to_owned());
                }
                if self.has_encoder("hevc_vaapi") {
                    out.push("hevc".to_owned());
                }
                if self.has_encoder("av1_vaapi") {
                    out.push("av1".to_owned());
                }
            }
            "qsv" => {
                if self.has_encoder("h264_qsv") {
                    out.push("h264".to_owned());
                }
                if self.has_encoder("hevc_qsv") {
                    out.push("hevc".to_owned());
                }
                if self.has_encoder("av1_qsv") {
                    out.push("av1".to_owned());
                }
            }
            "nvenc" => {
                if self.has_encoder("h264_nvenc") {
                    out.push("h264".to_owned());
                }
                if self.has_encoder("hevc_nvenc") {
                    out.push("hevc".to_owned());
                }
                if self.has_encoder("av1_nvenc") {
                    out.push("av1".to_owned());
                }
            }
            "d3d11Amf" => {
                if self.has_encoder("h264_amf") {
                    out.push("h264".to_owned());
                }
                if self.has_encoder("hevc_amf") {
                    out.push("hevc".to_owned());
                }
                if self.has_encoder("av1_amf") {
                    out.push("av1".to_owned());
                }
            }
            "v4l2m2m" => {
                if self.has_encoder("h264_v4l2m2m") {
                    out.push("h264".to_owned());
                }
            }
            "piDrmDecode" | "software" => {
                if self.has_encoder("libx264") || self.encoders.is_empty() {
                    out.push("h264".to_owned());
                }
                if self.has_encoder("libx265") {
                    out.push("hevc".to_owned());
                }
                if self.has_encoder("libsvtav1") {
                    out.push("av1".to_owned());
                }
            }
            _ => {}
        }
        out
    }

    /// Whether HDR tone mapping is supported for the given backend and platform profile (M4.10).
    /// Software tone mapping is explicitly disabled on Raspberry Pi and generic ARM SBC profiles.
    pub fn can_tonemap_on_backend(&self, backend: &str, profile: PlatformProfile) -> bool {
        if profile.is_pi_or_arm() {
            return false;
        }
        match backend {
            "vaapi" => self.supports_tonemap_vaapi || self.supports_tonemap_opencl,
            "qsv" => self.supports_vpp_qsv || self.supports_tonemap_opencl,
            "nvenc" => self.supports_tonemap_cuda || self.supports_libplacebo,
            "d3d11Amf" => self.supports_libplacebo || self.supports_zscale_tonemap,
            "software" => self.supports_zscale_tonemap || self.supports_libplacebo,
            _ => false,
        }
    }
}

static PROBE_CACHE: LazyLock<RwLock<HashMap<String, FfmpegProbeCaps>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Compute a fast xxh3-64 hash of the FFmpeg binary file (or its path/mtime metadata if large).
pub fn hash_ffmpeg_binary(ffmpeg_bin: &Path) -> String {
    if let Ok(bytes) = std::fs::read(ffmpeg_bin) {
        return format!("{:016x}", xxh3_64(&bytes));
    }
    if let Ok(meta) = std::fs::metadata(ffmpeg_bin) {
        let len = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let key = format!("{}:{len}:{mtime}", ffmpeg_bin.display());
        return format!("{:016x}", xxh3_64(key.as_bytes()));
    }
    "missing-ffmpeg".to_owned()
}

/// Probe `ffmpeg_bin` for `-version`, `-hwaccels`, `-encoders`, `-decoders`, and `-filters`,
/// caching the result by binary hash.
pub fn probe_ffmpeg(ffmpeg_bin: &Path) -> FfmpegProbeCaps {
    let binary_hash = hash_ffmpeg_binary(ffmpeg_bin);
    if let Ok(cache) = PROBE_CACHE.read()
        && let Some(found) = cache.get(&binary_hash)
    {
        return found.clone();
    }

    let version_out = run_ffmpeg_flag(ffmpeg_bin, "-version");
    let hwaccels_out = run_ffmpeg_flag(ffmpeg_bin, "-hwaccels");
    let encoders_out = run_ffmpeg_flag(ffmpeg_bin, "-encoders");
    let decoders_out = run_ffmpeg_flag(ffmpeg_bin, "-decoders");
    let filters_out = run_ffmpeg_flag(ffmpeg_bin, "-filters");

    let caps = parse_from_outputs(
        binary_hash.clone(),
        &version_out,
        &hwaccels_out,
        &encoders_out,
        &decoders_out,
        &filters_out,
    );

    if binary_hash != "missing-ffmpeg"
        && let Ok(mut cache) = PROBE_CACHE.write()
    {
        cache.insert(binary_hash, caps.clone());
    }
    caps
}

fn run_ffmpeg_flag(ffmpeg_bin: &Path, flag: &str) -> String {
    Command::new(ffmpeg_bin)
        .args(["-hide_banner", flag])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

/// Pure parser turning raw `ffmpeg` CLI outputs into [`FfmpegProbeCaps`].
pub fn parse_from_outputs(
    binary_hash: String,
    version_out: &str,
    hwaccels_out: &str,
    encoders_out: &str,
    decoders_out: &str,
    filters_out: &str,
) -> FfmpegProbeCaps {
    let version = version_out.lines().next().map(str::trim).unwrap_or("unknown").to_owned();
    let hwaccels = parse_hwaccels(hwaccels_out);
    let encoders = parse_codecs(encoders_out);
    let decoders = parse_codecs(decoders_out);
    let filters = parse_filters(filters_out);

    let has_f = |name: &str| filters.iter().any(|f| f == name);
    let supports_tonemap_vaapi = has_f("tonemap_vaapi");
    let supports_tonemap_cuda = has_f("tonemap_cuda");
    let supports_tonemap_opencl = has_f("tonemap_opencl");
    let supports_libplacebo = has_f("libplacebo");
    let supports_vpp_qsv = has_f("vpp_qsv");
    let supports_zscale_tonemap = has_f("zscale") && has_f("tonemap");

    FfmpegProbeCaps {
        binary_hash,
        version,
        hwaccels,
        encoders,
        decoders,
        filters,
        supports_tonemap_vaapi,
        supports_tonemap_cuda,
        supports_tonemap_opencl,
        supports_libplacebo,
        supports_vpp_qsv,
        supports_zscale_tonemap,
    }
}

/// Parse `ffmpeg -hwaccels` output.
pub fn parse_hwaccels(output: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut after_header = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("Hardware acceleration methods:") {
            after_header = true;
            continue;
        }
        if after_header && !trimmed.contains(' ') {
            out.push(trimmed.to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Parse `ffmpeg -encoders` or `ffmpeg -decoders` output, extracting only video/audio/subtitle codec names.
pub fn parse_codecs(output: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut after_sep = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("------") {
            after_sep = true;
            continue;
        }
        if !after_sep || trimmed.len() < 8 {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(flags) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        if flags.len() == 6 && !name.is_empty() && name != "=" {
            out.push(name.to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Parse `ffmpeg -filters` output, extracting filter names.
pub fn parse_filters(output: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("Filters:")
            || trimmed.starts_with("T..")
            || trimmed.starts_with(".S.")
            || trimmed.starts_with("..C")
        {
            continue;
        }
        // Standard filter line format: ` T.. scale            V->V       Scale the input video...`
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 3
            && parts[0].len() <= 4
            && parts[0].chars().all(|c| matches!(c, '.' | 'T' | 'S' | 'C' | 'A'))
            && parts[2].contains("->")
        {
            out.push(parts[1].to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_VERSION: &str =
        "ffmpeg version 7.0.2-mbl Copyright (c) 2000-2024 the FFmpeg developers\nbuilt with gcc 13";
    const SAMPLE_HWACCELS_INTEL_NV: &str = "\
Hardware acceleration methods:
vdpau
cuda
vaapi
qsv
drm
opencl
vulkan
";
    const SAMPLE_HWACCELS_PI: &str = "\
Hardware acceleration methods:
drm
";
    const SAMPLE_ENCODERS: &str = "\
Encoders:
 V..... = Video
 A..... = Audio
 S..... = Subtitle
 ------
 V....D libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V....D libx265              libx265 H.265 / HEVC (codec hevc)
 V....D libsvtav1            SVT-AV1(Scalable Video Technology for AV1) encoder (codec av1)
 V....D h264_vaapi           H.264/AVC (VAAPI) (codec h264)
 V....D hevc_vaapi           H.265/HEVC (VAAPI) (codec hevc)
 V....D av1_vaapi            AV1 (VAAPI) (codec av1)
 V....D h264_qsv             H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (Intel Quick Sync Video acceleration) (codec h264)
 V....D hevc_qsv             HEVC (Intel Quick Sync Video acceleration) (codec hevc)
 V....D av1_qsv              AV1 (Intel Quick Sync Video acceleration) (codec av1)
 V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)
 V....D hevc_nvenc           NVIDIA NVENC hevc encoder (codec hevc)
 V....D av1_nvenc            NVIDIA NVENC av1 encoder (codec av1)
 V....D h264_v4l2m2m         V4L2 mem2mem H.264 encoder wrapper (codec h264)
 A....D aac                  AAC (Advanced Audio Coding)
 A....D libopus              libopus Opus (codec opus)
";
    const SAMPLE_DECODERS: &str = "\
Decoders:
 V..... = Video
 ------
 V....D h264                 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10
 V....D hevc                 HEVC (High Efficiency Video Coding)
 V....D hevc_v4l2m2m         V4L2 mem2mem HEVC decoder wrapper (codec hevc)
 V....D h264_cuvid           Nvidia CUVID H264 decoder (codec h264)
 V....D hevc_qsv             HEVC video (Intel Quick Sync Video acceleration) (codec hevc)
";
    const SAMPLE_FILTERS: &str = "\
Filters:
  T.. = Timeline support
  .S. = Slice threading
  ..C = Command support
 ... scale             V->V       Scale the input video size and/or convert the image format.
 ... scale_vaapi       V->V       Scale to/from VAAPI surfaces.
 ... deinterlace_vaapi V->V       Deinterlace VAAPI surfaces.
 ... tonemap_vaapi     V->V       VAAPI VPP for tone-mapping.
 ... vpp_qsv           V->V       Quick Sync Video VPP.
 ... scale_cuda        V->V       GPU accelerated video resizer.
 ... yadif_cuda        V->V       Deinterlace CUDA frames.
 ... tonemap_cuda      V->V       GPU accelerated HDR to SDR tonemapping.
 ... tonemap_opencl    V->V       OpenCL tone-mapping.
 ... libplacebo        V->V       Apply various GPU filters from libplacebo.
 ... zscale            V->V       Apply resizing, colorspace and bit depth conversion.
 ... tonemap           V->V       Conversion to/from different dynamic ranges.
";

    #[test]
    fn parses_full_gpu_build_outputs() {
        let caps = parse_from_outputs(
            "0123456789abcdef".to_owned(),
            SAMPLE_VERSION,
            SAMPLE_HWACCELS_INTEL_NV,
            SAMPLE_ENCODERS,
            SAMPLE_DECODERS,
            SAMPLE_FILTERS,
        );
        assert_eq!(caps.hwaccels, vec!["cuda", "drm", "opencl", "qsv", "vaapi", "vdpau", "vulkan"]);
        assert!(caps.has_encoder("h264_vaapi"));
        assert!(caps.has_encoder("hevc_nvenc"));
        assert!(caps.has_encoder("av1_qsv"));
        assert!(caps.has_decoder("hevc_v4l2m2m"));
        assert!(caps.supports_tonemap_vaapi);
        assert!(caps.supports_tonemap_cuda);
        assert!(caps.supports_tonemap_opencl);
        assert!(caps.supports_libplacebo);
        assert!(caps.supports_vpp_qsv);
        assert!(caps.supports_zscale_tonemap);

        assert_eq!(caps.supported_encode_codecs_for_backend("vaapi"), vec!["h264", "hevc", "av1"]);
        assert!(caps.can_tonemap_on_backend("vaapi", PlatformProfile::X86Intel));
        assert!(caps.can_tonemap_on_backend("nvenc", PlatformProfile::X86Nvidia));
        assert!(!caps.can_tonemap_on_backend("software", PlatformProfile::Pi5));
    }

    #[test]
    fn parses_pi_build_outputs() {
        let caps = parse_from_outputs(
            "fedcba9876543210".to_owned(),
            SAMPLE_VERSION,
            SAMPLE_HWACCELS_PI,
            SAMPLE_ENCODERS,
            SAMPLE_DECODERS,
            SAMPLE_FILTERS,
        );
        assert_eq!(caps.hwaccels, vec!["drm"]);
        assert_eq!(caps.supported_encode_codecs_for_backend("v4l2m2m"), vec!["h264"]);
        assert!(!caps.can_tonemap_on_backend("piDrmDecode", PlatformProfile::Pi5));
    }
}
