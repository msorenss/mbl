//! Loudness analysis (`ebur128`), ReplayGain, and gapless audio metadata (M7.2).

use mbl_core::{Error, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LoudnessResult {
    /// Integrated loudness in LUFS (e.g. -14.2 LUFS)
    pub integrated_lufs: f64,
    /// Loudness Range in LU (e.g. 7.5 LU)
    pub loudness_range: f64,
    /// Maximum True Peak in dBTP (e.g. -0.8 dBTP)
    pub true_peak: f64,
}

/// Build FFmpeg command arguments for running `ebur128` loudness analysis.
pub fn build_ebur128_ffmpeg_args(input_path: &Path) -> Vec<String> {
    vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-v".into(),
        "info".into(),
        "-i".into(),
        input_path.to_string_lossy().into_owned(),
        "-af".into(),
        "ebur128=peak=true".into(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ]
}

/// Parse FFmpeg `ebur128` filter summary block from stderr.
pub fn parse_ebur128_output(stderr: &str) -> Option<LoudnessResult> {
    let mut integrated_lufs = None;
    let mut loudness_range = None;
    let mut true_peak = None;

    let mut in_summary = false;
    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Summary:") {
            in_summary = true;
            continue;
        }
        if !in_summary {
            continue;
        }

        if trimmed.starts_with("I:") && trimmed.contains("LUFS") {
            // e.g. "I: -16.4 LUFS"
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                integrated_lufs = parts[1].parse::<f64>().ok();
            }
        } else if trimmed.starts_with("LRA:") && trimmed.contains("LU") {
            // e.g. "LRA: 7.2 LU"
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                loudness_range = parts[1].parse::<f64>().ok();
            }
        } else if trimmed.starts_with("Peak:") && trimmed.contains("dBFS") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                true_peak = parts[1].parse::<f64>().ok();
            }
        } else if trimmed.starts_with("True peak:") && trimmed.contains("dBTP") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                true_peak = parts[2].parse::<f64>().ok();
            }
        }
    }

    Some(LoudnessResult {
        integrated_lufs: integrated_lufs?,
        loudness_range: loudness_range.unwrap_or(0.0),
        true_peak: true_peak.unwrap_or(0.0),
    })
}

/// Execute FFmpeg ebur128 loudness analysis on an audio or video file.
pub async fn analyze_loudness(ffmpeg_path: &Path, input_path: &Path) -> Result<LoudnessResult> {
    let args = build_ebur128_ffmpeg_args(input_path);
    let output = Command::new(ffmpeg_path)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| Error::External(format!("failed to run ffmpeg ebur128: {e}")))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_ebur128_output(&stderr)
        .ok_or_else(|| Error::External(format!("failed to parse ebur128 summary: {stderr}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_ebur128_summary() {
        let sample = r#"
[Parsed_ebur128_0 @ 0x55dc1b0] Summary:

  Integrated loudness:
    I:         -18.4 LUFS
    Threshold: -28.4 LUFS

  Loudness range:
    LRA:         6.8 LU
    Threshold: -38.4 LUFS
    LRA low:   -22.1 LUFS
    LRA high:  -15.3 LUFS

  True peak:
    Peak:       -1.2 dBFS
    True peak:  -0.9 dBTP
"#;
        let res = parse_ebur128_output(sample).expect("failed to parse sample");
        assert_eq!(res.integrated_lufs, -18.4);
        assert_eq!(res.loudness_range, 6.8);
        assert_eq!(res.true_peak, -0.9);
    }
}
