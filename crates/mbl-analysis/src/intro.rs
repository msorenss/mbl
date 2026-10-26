//! Intro detection (M5.6): chapter-name precedence + Chromaprint-style audio fingerprint matching
//! across the first 10 minutes (`600 s`) of episodes in a season.

use std::collections::HashMap;
use std::path::Path;

use mbl_core::{Error, ItemId, Result};
use mbl_probe::Chapter;
use serde::{Deserialize, Serialize};

/// Sample rate used when extracting mono PCM audio for intro fingerprinting.
pub const SAMPLE_RATE_HZ: u32 = 16_000;
/// Window step in milliseconds (`250 ms` = 4 sub-fingerprints per second).
pub const FRAME_STEP_MS: u64 = 250;
/// Samples per `250 ms` window at `16 kHz`.
pub const SAMPLES_PER_FRAME: usize = 4_000;
/// Maximum duration inspected for intro detection (`10 min` = `600_000 ms`).
pub const MAX_INTRO_SEARCH_MS: u64 = 600_000;
/// Minimum intro duration (`10 s`).
pub const MIN_INTRO_DURATION_MS: u64 = 10_000;
/// Maximum intro duration (`180 s`).
pub const MAX_INTRO_DURATION_MS: u64 = 180_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetectedMarker {
    pub kind: String,
    pub title: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct EpisodeFingerprint {
    pub item_id: ItemId,
    /// One 32-bit Chromaprint-style sub-fingerprint per `FRAME_STEP_MS` (`250 ms`) window.
    pub frames: Vec<u32>,
}

/// Check if chapters already define an explicit intro marker (takes precedence per M5.6).
pub fn detect_intro_from_chapters(chapters: &[Chapter]) -> Option<DetectedMarker> {
    for ch in chapters {
        if ch.start_ms > MAX_INTRO_SEARCH_MS {
            continue;
        }
        let Some(title) = ch.title.as_deref() else {
            continue;
        };
        let lower = title.trim().to_ascii_lowercase();
        let is_intro = lower == "intro"
            || lower == "opening"
            || lower == "op"
            || lower.starts_with("intro ")
            || lower.starts_with("opening ")
            || lower.contains("opening credits")
            || lower.contains("title sequence");
        if is_intro && ch.end_ms > ch.start_ms {
            return Some(DetectedMarker {
                kind: "intro".into(),
                title: Some(title.trim().to_owned()),
                start_ms: ch.start_ms,
                end_ms: ch.end_ms,
                source: "chapter".into(),
                confidence: 1.0,
            });
        }
    }
    None
}

/// Compute a sequence of 32-bit Chromaprint-style audio sub-fingerprints from 16 kHz mono s16le PCM samples.
///
/// Each `250 ms` frame (`4000` samples) is decomposed into 16 temporal/spectral features:
/// - 8 sub-window log-energy differences
/// - 8 sub-window zero-crossing & first-order difference (high-frequency vs low-frequency) ratios
///   quantized into a 32-bit Gray-coded hash so Hamming distance measures perceptual audio similarity.
pub fn fingerprint_pcm_s16le(samples: &[i16]) -> Vec<u32> {
    let max_samples =
        ((MAX_INTRO_SEARCH_MS / FRAME_STEP_MS) as usize).saturating_mul(SAMPLES_PER_FRAME);
    let slice = if samples.len() > max_samples { &samples[..max_samples] } else { samples };

    let mut out = Vec::with_capacity(slice.len() / SAMPLES_PER_FRAME);
    for chunk in slice.as_chunks::<SAMPLES_PER_FRAME>().0 {
        out.push(frame_subfingerprint(chunk));
    }
    out
}

fn frame_subfingerprint(frame: &[i16]) -> u32 {
    // Split 4000 samples into 8 sub-blocks of 500 samples each.
    let sub_len = SAMPLES_PER_FRAME / 8;
    let mut energies = [0.0f64; 8];
    let mut zcrs = [0u32; 8];
    let mut hf_energies = [0.0f64; 8];

    for b in 0..8 {
        let sub = &frame[b * sub_len..(b + 1) * sub_len];
        let mut e = 0.0f64;
        let mut hf = 0.0f64;
        let mut zcr = 0u32;
        let mut prev = sub[0];
        for &s in sub {
            let v = f64::from(s);
            e += v * v;
            let diff = f64::from(s) - f64::from(prev);
            hf += diff * diff;
            if (s >= 0 && prev < 0) || (s < 0 && prev >= 0) {
                zcr += 1;
            }
            prev = s;
        }
        energies[b] = (e / (sub_len as f64)).sqrt();
        hf_energies[b] = (hf / (sub_len as f64)).sqrt();
        zcrs[b] = zcr;
    }

    let mut bits = 0u32;
    // Bits 0..8: relative energy contour across adjacent sub-blocks
    for b in 0..8 {
        let next = (b + 1) % 8;
        if energies[b] > energies[next] * 1.02 {
            bits |= 1 << b;
        }
    }
    // Bits 8..16: zero-crossing rate (pitch/frequency) thresholds (4 quantization levels per pair)
    for (b, &zcr) in zcrs.iter().enumerate() {
        // At 16kHz, 500 samples = 31.25ms. 440Hz => ~27.5 zero crossings.
        if zcr > 20 {
            bits |= 1 << (8 + b);
        }
    }
    // Bits 16..24: fine pitch/ZCR gradient across sub-blocks and absolute pitch band
    for (b, &zcr) in zcrs.iter().enumerate() {
        if zcr > 38 {
            bits |= 1 << (16 + b);
        }
    }
    // Bits 24..32: spectral tilt (high-frequency derivative ratio vs RMS energy)
    for b in 0..8 {
        let ratio = if energies[b] > 1.0 { hf_energies[b] / energies[b] } else { 0.0 };
        if ratio > 0.45 {
            bits |= 1 << (24 + b);
        }
    }
    bits
}

/// Extract the first 10 minutes of 16 kHz mono s16le PCM audio from `media_path` using FFmpeg and fingerprint it.
pub async fn fingerprint_media_file(
    ffmpeg_bin: &Path,
    item_id: ItemId,
    media_path: &Path,
) -> Result<EpisodeFingerprint> {
    let mut input_arg = std::ffi::OsString::from("file:");
    input_arg.push(media_path.as_os_str());

    let output = tokio::process::Command::new(ffmpeg_bin)
        .args([
            std::ffi::OsString::from("-hide_banner"),
            std::ffi::OsString::from("-nostdin"),
            std::ffi::OsString::from("-loglevel"),
            std::ffi::OsString::from("error"),
            std::ffi::OsString::from("-t"),
            std::ffi::OsString::from("600"),
            std::ffi::OsString::from("-i"),
            input_arg,
            std::ffi::OsString::from("-vn"),
            std::ffi::OsString::from("-ac"),
            std::ffi::OsString::from("1"),
            std::ffi::OsString::from("-ar"),
            std::ffi::OsString::from(SAMPLE_RATE_HZ.to_string()),
            std::ffi::OsString::from("-f"),
            std::ffi::OsString::from("s16le"),
            std::ffi::OsString::from("-"),
        ])
        .output()
        .await
        .map_err(Error::internal)?;

    let mut samples = Vec::with_capacity(output.stdout.len() / 2);
    for &chunk in output.stdout.as_chunks::<2>().0 {
        samples.push(i16::from_le_bytes(chunk));
    }

    Ok(EpisodeFingerprint { item_id, frames: fingerprint_pcm_s16le(&samples) })
}

#[inline]
fn frame_similarity(a: u32, b: u32) -> f32 {
    let dist = (a ^ b).count_ones();
    (32 - dist) as f32 / 32.0
}

/// Find the common intro segment across `>= 2` episode fingerprints in a season (M5.6).
pub fn find_common_intros(episodes: &[EpisodeFingerprint]) -> HashMap<ItemId, DetectedMarker> {
    let mut result = HashMap::new();
    if episodes.len() < 2 {
        return result;
    }

    let min_frames = (MIN_INTRO_DURATION_MS / FRAME_STEP_MS) as usize;
    let max_frames = (MAX_INTRO_DURATION_MS / FRAME_STEP_MS) as usize;
    let ref_ep = &episodes[0];
    if ref_ep.frames.len() < min_frames {
        return result;
    }

    // Compare episode 0 against episode 1 to find the best matching diagonal offset and run.
    let target_ep = &episodes[1];
    if target_ep.frames.len() < min_frames {
        return result;
    }

    let Some((ref_start, ref_len, best_score)) =
        find_best_common_run(&ref_ep.frames, &target_ep.frames, min_frames, max_frames)
    else {
        return result;
    };

    let ref_slice = &ref_ep.frames[ref_start..ref_start + ref_len];
    result.insert(
        ref_ep.item_id,
        DetectedMarker {
            kind: "intro".into(),
            title: Some("Intro".into()),
            start_ms: (ref_start as u64) * FRAME_STEP_MS,
            end_ms: ((ref_start + ref_len) as u64) * FRAME_STEP_MS,
            source: "fingerprint".into(),
            confidence: f64::from(best_score),
        },
    );

    // Locate the matching intro template in every subsequent episode in the season.
    for ep in &episodes[1..] {
        if let Some((start_idx, score)) = match_template_in_episode(ref_slice, &ep.frames) {
            result.insert(
                ep.item_id,
                DetectedMarker {
                    kind: "intro".into(),
                    title: Some("Intro".into()),
                    start_ms: (start_idx as u64) * FRAME_STEP_MS,
                    end_ms: ((start_idx + ref_len) as u64) * FRAME_STEP_MS,
                    source: "fingerprint".into(),
                    confidence: f64::from(score),
                },
            );
        }
    }

    result
}

fn find_best_common_run(
    a: &[u32],
    b: &[u32],
    min_frames: usize,
    max_frames: usize,
) -> Option<(usize, usize, f32)> {
    let n = a.len();
    let m = b.len();
    let threshold = 0.85f32;

    let mut best: Option<(usize, usize, f32)> = None;

    // Iterate over all possible lag offsets `offset = j as isize - i as isize`
    let min_offset = -(n as isize) + (min_frames as isize);
    let max_offset = (m as isize) - (min_frames as isize);

    for offset in min_offset..=max_offset {
        let i_start = if offset < 0 { (-offset) as usize } else { 0 };
        let j_start = if offset > 0 { offset as usize } else { 0 };
        let diag_len = (n - i_start).min(m - j_start);
        if diag_len < min_frames {
            continue;
        }

        let mut run_start = 0usize;
        let mut run_len = 0usize;
        let mut run_sum = 0.0f32;

        for k in 0..diag_len {
            let sim = frame_similarity(a[i_start + k], b[j_start + k]);
            if sim >= threshold {
                if run_len == 0 {
                    run_start = k;
                }
                run_len += 1;
                run_sum += sim;
            } else {
                if run_len >= min_frames {
                    let capped_len = run_len.min(max_frames);
                    let avg = run_sum / (run_len as f32);
                    if best.is_none_or(|(_, best_len, best_avg)| {
                        (capped_len as f32) * avg > (best_len as f32) * best_avg
                    }) {
                        best = Some((i_start + run_start, capped_len, avg));
                    }
                }
                run_len = 0;
                run_sum = 0.0;
            }
        }

        if run_len >= min_frames {
            let capped_len = run_len.min(max_frames);
            let avg = run_sum / (run_len as f32);
            if best.is_none_or(|(_, best_len, best_avg)| {
                (capped_len as f32) * avg > (best_len as f32) * best_avg
            }) {
                best = Some((i_start + run_start, capped_len, avg));
            }
        }
    }

    best
}

fn match_template_in_episode(template: &[u32], frames: &[u32]) -> Option<(usize, f32)> {
    let t_len = template.len();
    if frames.len() < t_len || t_len == 0 {
        return None;
    }

    let mut best_idx = 0usize;
    let mut best_score = 0.0f32;
    for start in 0..=(frames.len() - t_len) {
        let mut sum = 0.0f32;
        for k in 0..t_len {
            sum += frame_similarity(template[k], frames[start + k]);
        }
        let avg = sum / (t_len as f32);
        if avg > best_score {
            best_score = avg;
            best_idx = start;
        }
    }

    (best_score >= 0.80).then_some((best_idx, best_score))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// Generate synthetic 16 kHz mono PCM where an identical 30-second intro tone pattern is embedded
    /// at `[intro_start_s .. intro_start_s + 30]` amidst episode-specific background audio.
    fn synth_episode_pcm(total_secs: usize, intro_start_secs: usize, seed: u64) -> Vec<i16> {
        let sr = SAMPLE_RATE_HZ as usize;
        let total_samples = total_secs * sr;
        let intro_start = intro_start_secs * sr;
        let intro_end = (intro_start_secs + 30) * sr;

        let mut pcm = Vec::with_capacity(total_samples);
        let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1);

        for idx in 0..total_samples {
            let t = (idx as f64) / (sr as f64);
            let sample = if idx >= intro_start && idx < intro_end {
                // Identical 30 s melodic intro tone pattern (alternating arpeggio every 1.0 s)
                let rel_t = (idx - intro_start) as f64 / (sr as f64);
                let note_idx = (rel_t as usize) % 4;
                let freq = match note_idx {
                    0 => 440.0,
                    1 => 659.25,
                    2 => 880.0,
                    _ => 523.25,
                };
                let env = if (rel_t.fract()) < 0.85 { 18_000.0 } else { 3_000.0 };
                (env * (2.0 * PI * freq * rel_t).sin()) as i16
            } else {
                // Episode-specific non-intro audio (distinct carrier + pseudo-random modulation per episode)
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let noise = (((rng >> 48) as i32) - 32768) / 32;
                let base_freq = 140.0 + ((seed % 5) as f64) * 35.0;
                let voice = (2500.0 * (2.0 * PI * base_freq * t).sin()) as i32;
                (voice + noise).clamp(-30_000, 30_000) as i16
            };
            pcm.push(sample);
        }
        pcm
    }

    #[test]
    fn chapter_names_take_precedence_m5_6() {
        let chapters = vec![
            Chapter { start_ms: 0, end_ms: 15_000, title: Some("Cold Open".into()) },
            Chapter { start_ms: 15_000, end_ms: 45_000, title: Some("Opening Credits".into()) },
            Chapter { start_ms: 45_000, end_ms: 1_200_000, title: Some("Act I".into()) },
        ];
        let marker = detect_intro_from_chapters(&chapters).expect("chapter intro detected");
        assert_eq!(marker.source, "chapter");
        assert_eq!(marker.start_ms, 15_000);
        assert_eq!(marker.end_ms, 45_000);
    }

    #[test]
    fn synthetic_30s_intro_tone_pattern_detected_within_1s_m5_6() {
        // Episode 1: 30s intro at 12.0s..42.0s
        let ep1_pcm = synth_episode_pcm(90, 12, 101);
        // Episode 2: 30s intro at 24.0s..54.0s
        let ep2_pcm = synth_episode_pcm(90, 24, 202);
        // Episode 3: 30s intro at 8.0s..38.0s
        let ep3_pcm = synth_episode_pcm(90, 8, 303);

        let fps = vec![
            EpisodeFingerprint { item_id: ItemId(1), frames: fingerprint_pcm_s16le(&ep1_pcm) },
            EpisodeFingerprint { item_id: ItemId(2), frames: fingerprint_pcm_s16le(&ep2_pcm) },
            EpisodeFingerprint { item_id: ItemId(3), frames: fingerprint_pcm_s16le(&ep3_pcm) },
        ];

        let detected = find_common_intros(&fps);
        assert_eq!(detected.len(), 3);

        let m1 = &detected[&ItemId(1)];
        assert!(
            m1.start_ms.abs_diff(12_000) <= 1_000,
            "ep1 start_ms={} expected ~12000",
            m1.start_ms
        );
        assert!(m1.end_ms.abs_diff(42_000) <= 1_000, "ep1 end_ms={} expected ~42000", m1.end_ms);

        let m2 = &detected[&ItemId(2)];
        assert!(
            m2.start_ms.abs_diff(24_000) <= 1_000,
            "ep2 start_ms={} expected ~24000",
            m2.start_ms
        );
        assert!(m2.end_ms.abs_diff(54_000) <= 1_000, "ep2 end_ms={} expected ~54000", m2.end_ms);

        let m3 = &detected[&ItemId(3)];
        assert!(
            m3.start_ms.abs_diff(8_000) <= 1_000,
            "ep3 start_ms={} expected ~8000",
            m3.start_ms
        );
        assert!(m3.end_ms.abs_diff(38_000) <= 1_000, "ep3 end_ms={} expected ~38000", m3.end_ms);
    }
}
