//! Optimized media versions (M5.4): "Universal 1080p" and "Universal 720p" (H.264 High + AAC stereo MP4),
//! night-window scheduling helper, dry-run size/time estimation, and FFmpeg MP4 generator.

use std::ffi::OsString;
use std::path::Path;

use mbl_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Pre-transcoded optimization target profiles (M5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationProfile {
    Universal1080p,
    Universal720p,
}

impl OptimizationProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Universal1080p => "universal_1080p",
            Self::Universal720p => "universal_720p",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "universal_1080p" | "1080p" | "universal 1080p" => Some(Self::Universal1080p),
            "universal_720p" | "720p" | "universal 720p" => Some(Self::Universal720p),
            _ => None,
        }
    }

    pub const fn max_width(self) -> u32 {
        match self {
            Self::Universal1080p => 1920,
            Self::Universal720p => 1280,
        }
    }

    pub const fn max_height(self) -> u32 {
        match self {
            Self::Universal1080p => 1080,
            Self::Universal720p => 720,
        }
    }

    pub const fn video_bitrate_bps(self) -> u64 {
        match self {
            Self::Universal1080p => 8_000_000,
            Self::Universal720p => 4_000_000,
        }
    }

    pub const fn audio_bitrate_bps(self) -> u64 {
        match self {
            Self::Universal1080p => 192_000,
            Self::Universal720p => 160_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationEstimate {
    pub profile: OptimizationProfile,
    pub estimated_size_bytes: u64,
    pub estimated_duration_secs: u64,
}

/// Estimate output file size and encoding time for a `dry_run` preview (M5.4 / PLAN §9.5.3).
pub fn estimate_optimization(
    duration_ms: u64,
    profile: OptimizationProfile,
) -> OptimizationEstimate {
    let secs = (duration_ms / 1000).max(1);
    let total_bps = profile.video_bitrate_bps() + profile.audio_bitrate_bps();
    let estimated_size_bytes = (secs * total_bps) / 8;
    // Assume ~1.5x realtime on typical hardware
    let estimated_duration_secs = (secs * 2 / 3).max(1);
    OptimizationEstimate { profile, estimated_size_bytes, estimated_duration_secs }
}

/// Check whether `current_hour` (`0..=23`) falls inside `[start_hour, end_hour)` (supports wrap-around e.g. `1..6` or `23..6`).
pub const fn is_within_night_window(current_hour: u8, start_hour: u8, end_hour: u8) -> bool {
    if start_hour == end_hour {
        return true;
    }
    if start_hour < end_hour {
        current_hour >= start_hour && current_hour < end_hour
    } else {
        current_hour >= start_hour || current_hour < end_hour
    }
}

/// Build FFmpeg arguments to transcode `input` into a universal H.264 High + AAC stereo MP4 (`+faststart`).
pub fn build_optimize_ffmpeg_args(
    input: &Path,
    output: &Path,
    profile: OptimizationProfile,
) -> Vec<OsString> {
    let mut input_arg = OsString::from("file:");
    input_arg.push(input.as_os_str());
    let vf = format!(
        "scale='min({},iw)':-2:force_original_aspect_ratio=decrease,format=yuv420p",
        profile.max_width()
    );
    let v_bitrate = profile.video_bitrate_bps().to_string();
    let a_bitrate = profile.audio_bitrate_bps().to_string();

    vec![
        OsString::from("-hide_banner"),
        OsString::from("-nostdin"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-y"),
        OsString::from("-i"),
        input_arg,
        OsString::from("-map"),
        OsString::from("0:v:0"),
        OsString::from("-map"),
        OsString::from("0:a:0?"),
        OsString::from("-vf"),
        OsString::from(vf),
        OsString::from("-c:v"),
        OsString::from("libx264"),
        OsString::from("-preset"),
        OsString::from("veryfast"),
        OsString::from("-profile:v"),
        OsString::from("high"),
        OsString::from("-level:v"),
        OsString::from("4.1"),
        OsString::from("-b:v"),
        OsString::from(v_bitrate),
        OsString::from("-c:a"),
        OsString::from("aac"),
        OsString::from("-ac"),
        OsString::from("2"),
        OsString::from("-b:a"),
        OsString::from(a_bitrate),
        OsString::from("-movflags"),
        OsString::from("+faststart"),
        output.as_os_str().to_owned(),
    ]
}

/// Run FFmpeg to produce the optimized MP4 version on disk.
pub async fn transcode_optimized_mp4(
    ffmpeg_bin: &Path,
    input: &Path,
    output: &Path,
    profile: OptimizationProfile,
) -> Result<u64> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(Error::internal)?;
    }
    let args = build_optimize_ffmpeg_args(input, output, profile);
    let out = tokio::process::Command::new(ffmpeg_bin)
        .args(&args)
        .output()
        .await
        .map_err(Error::internal)?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Internal(
            format!("FFmpeg optimize failed ({}): {}", out.status, stderr.trim()).into(),
        ));
    }

    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_window_wraps_and_matches_correctly() {
        assert!(is_within_night_window(2, 1, 6));
        assert!(!is_within_night_window(14, 1, 6));
        assert!(is_within_night_window(23, 22, 5));
        assert!(is_within_night_window(3, 22, 5));
        assert!(!is_within_night_window(12, 22, 5));
    }

    #[test]
    fn builds_universal_h264_aac_mp4_args() {
        let args = build_optimize_ffmpeg_args(
            Path::new("/media/source.mkv"),
            Path::new("/media/.mbl-optimized/movie-1080p.mp4"),
            OptimizationProfile::Universal1080p,
        );
        let strs: Vec<String> = args.iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert!(strs.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx264"));
        assert!(strs.windows(2).any(|w| w[0] == "-profile:v" && w[1] == "high"));
        assert!(strs.windows(2).any(|w| w[0] == "-c:a" && w[1] == "aac"));
        assert!(strs.windows(2).any(|w| w[0] == "-movflags" && w[1] == "+faststart"));
    }
}
