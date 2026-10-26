//! Credits detection (M5.7): chapters first, then `blackdetect` + `silencedetect` heuristics
//! in the last 10% of the media item.

use std::path::Path;

use mbl_core::{Error, Result};
use mbl_probe::Chapter;
use regex::Regex;

use crate::intro::DetectedMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalMs {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Detect end credits from embedded media chapters first, then `blackdetect` + `silencedetect` logs in the last 10% (M5.7).
pub fn detect_credits(
    duration_ms: u64,
    chapters: &[Chapter],
    ffmpeg_filter_log: Option<&str>,
) -> Option<DetectedMarker> {
    if let Some(ch_marker) = detect_credits_from_chapters(duration_ms, chapters) {
        return Some(ch_marker);
    }
    let log = ffmpeg_filter_log?;
    let tail_start_ms = duration_ms.saturating_mul(9) / 10;
    detect_credits_from_filter_log(duration_ms, tail_start_ms, log)
}

/// Check if chapters in the final 25% of the item identify the end credits.
pub fn detect_credits_from_chapters(
    duration_ms: u64,
    chapters: &[Chapter],
) -> Option<DetectedMarker> {
    let min_start = duration_ms.saturating_mul(3) / 4;
    for ch in chapters {
        if ch.start_ms < min_start {
            continue;
        }
        let Some(title) = ch.title.as_deref() else {
            continue;
        };
        let lower = title.trim().to_ascii_lowercase();
        let is_credits = lower == "credits"
            || lower == "end credits"
            || lower == "ending"
            || lower == "ed"
            || lower == "outro"
            || lower.starts_with("credits ")
            || lower.contains("end credits")
            || lower.contains("closing credits");
        if is_credits && ch.end_ms > ch.start_ms {
            return Some(DetectedMarker {
                kind: "credits".into(),
                title: Some(title.trim().to_owned()),
                start_ms: ch.start_ms,
                end_ms: ch.end_ms.max(duration_ms),
                source: "chapter".into(),
                confidence: 1.0,
            });
        }
    }
    None
}

/// Parse FFmpeg `blackdetect` and `silencedetect` filter output from the last 10% (`tail_offset_ms`) of a video.
///
///Note: timestamps emitted by FFmpeg when started with `-ss <tail_offset>` are relative to `0.0` unless `-copyts` is used;
/// if parsed timestamps are `< tail_offset_ms`, `tail_offset_ms` is added automatically.
pub fn detect_credits_from_filter_log(
    duration_ms: u64,
    tail_offset_ms: u64,
    log: &str,
) -> Option<DetectedMarker> {
    if duration_ms == 0 {
        return None;
    }
    let black_re =
        Regex::new(r"black_start:\s*([0-9]+(?:\.[0-9]+)?)\s+black_end:\s*([0-9]+(?:\.[0-9]+)?)")
            .ok()?;
    let silence_start_re = Regex::new(r"silence_start:\s*([0-9]+(?:\.[0-9]+)?)").ok()?;
    let silence_end_re = Regex::new(r"silence_end:\s*([0-9]+(?:\.[0-9]+)?)").ok()?;

    let normalize = |raw_ms: u64| -> u64 {
        if raw_ms < tail_offset_ms { tail_offset_ms.saturating_add(raw_ms) } else { raw_ms }
    };

    let mut blacks = Vec::new();
    for cap in black_re.captures_iter(log) {
        let s_sec: f64 = cap.get(1)?.as_str().parse().ok()?;
        let e_sec: f64 = cap.get(2)?.as_str().parse().ok()?;
        let start_ms = normalize((s_sec * 1000.0).round() as u64);
        let end_ms = normalize((e_sec * 1000.0).round() as u64);
        if start_ms >= tail_offset_ms && end_ms > start_ms {
            blacks.push(IntervalMs { start_ms, end_ms });
        }
    }

    let mut silences = Vec::new();
    let mut pending_silence_start: Option<u64> = None;
    for line in log.lines() {
        if let Some(cap) = silence_start_re.captures(line)
            && let Ok(s_sec) = cap[1].parse::<f64>()
        {
            pending_silence_start = Some(normalize((s_sec * 1000.0).round() as u64));
        }
        if let Some(cap) = silence_end_re.captures(line)
            && let Ok(e_sec) = cap[1].parse::<f64>()
            && let Some(start_ms) = pending_silence_start.take()
        {
            let end_ms = normalize((e_sec * 1000.0).round() as u64);
            if start_ms >= tail_offset_ms && end_ms > start_ms {
                silences.push(IntervalMs { start_ms, end_ms });
            }
        }
    }

    // Prefer a black interval in the final 10% that overlaps or is within 3s of a silence interval.
    for b in &blacks {
        let matches_silence = silences.iter().any(|s| {
            b.start_ms.abs_diff(s.start_ms) <= 3_000
                || (b.start_ms <= s.end_ms && s.start_ms <= b.end_ms)
        });
        if matches_silence {
            return Some(DetectedMarker {
                kind: "credits".into(),
                title: Some("Credits".into()),
                start_ms: b.start_ms,
                end_ms: duration_ms,
                source: "heuristic".into(),
                confidence: 0.9,
            });
        }
    }

    // Fallback: first sustained black frame transition (>= 500 ms) in the final 10%.
    if let Some(first_black) = blacks.iter().find(|b| b.end_ms.saturating_sub(b.start_ms) >= 500) {
        return Some(DetectedMarker {
            kind: "credits".into(),
            title: Some("Credits".into()),
            start_ms: first_black.start_ms,
            end_ms: duration_ms,
            source: "heuristic".into(),
            confidence: 0.75,
        });
    }

    None
}

/// Run FFmpeg `blackdetect` + `silencedetect` on the final 10% of `media_path` and detect end credits.
pub async fn run_credits_detection(
    ffmpeg_bin: &Path,
    media_path: &Path,
    duration_ms: u64,
    chapters: &[Chapter],
) -> Result<Option<DetectedMarker>> {
    if let Some(ch) = detect_credits_from_chapters(duration_ms, chapters) {
        return Ok(Some(ch));
    }
    if duration_ms < 20_000 {
        return Ok(None);
    }

    let tail_offset_ms = duration_ms.saturating_mul(9) / 10;
    let ss_secs = format!("{:.3}", (tail_offset_ms as f64) / 1000.0);
    let mut input_arg = std::ffi::OsString::from("file:");
    input_arg.push(media_path.as_os_str());

    let output = tokio::process::Command::new(ffmpeg_bin)
        .args([
            std::ffi::OsString::from("-hide_banner"),
            std::ffi::OsString::from("-nostdin"),
            std::ffi::OsString::from("-ss"),
            std::ffi::OsString::from(ss_secs),
            std::ffi::OsString::from("-i"),
            input_arg,
            std::ffi::OsString::from("-vf"),
            std::ffi::OsString::from("blackdetect=d=0.5:pix_th=0.10"),
            std::ffi::OsString::from("-af"),
            std::ffi::OsString::from("silencedetect=n=-35dB:d=0.5"),
            std::ffi::OsString::from("-f"),
            std::ffi::OsString::from("null"),
            std::ffi::OsString::from("-"),
        ])
        .output()
        .await
        .map_err(Error::internal)?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(detect_credits_from_filter_log(duration_ms, tail_offset_ms, &stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_chapters_take_precedence_m5_7() {
        let duration_ms = 3_600_000;
        let chapters = vec![
            Chapter { start_ms: 0, end_ms: 3_300_000, title: Some("Main Feature".into()) },
            Chapter { start_ms: 3_300_000, end_ms: 3_600_000, title: Some("End Credits".into()) },
        ];
        let log = "[blackdetect @ 0x1] black_start:10.0 black_end:12.0 black_duration:2.0\n";
        let m = detect_credits(duration_ms, &chapters, Some(log)).unwrap();
        assert_eq!(m.source, "chapter");
        assert_eq!(m.start_ms, 3_300_000);
        assert_eq!(m.end_ms, 3_600_000);
    }

    #[test]
    fn credits_detected_from_blackdetect_and_silencedetect_fixture_m5_7() {
        let duration_ms = 2_000_000; // 2000s; last 10% starts at 1800s (1_800_000 ms)
        // Relative timestamps from -ss 1800: black at +45.5s (1845.5s = 1_845_500ms) + coinciding silence
        let fixture_log = "\
[silencedetect @ 0x55] silence_start: 45.200
[blackdetect @ 0x56] black_start:45.500 black_end:47.100 black_duration:1.600
[silencedetect @ 0x55] silence_end: 47.300 | silence_duration: 2.100
";
        let m = detect_credits(duration_ms, &[], Some(fixture_log)).unwrap();
        assert_eq!(m.kind, "credits");
        assert_eq!(m.source, "heuristic");
        assert_eq!(m.start_ms, 1_845_500);
        assert_eq!(m.end_ms, 2_000_000);
        assert!(m.confidence >= 0.85);
    }
}
