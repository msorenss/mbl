//! Auto-Sync Subtitles (`mbl-subsync`) alignment engine (M7.17).
//! Computes audio voice activity energy envelope and aligns out-of-sync SRT/VTT subtitles.

#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleCue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpeechSegment {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Parse SRT subtitle string into structured cues.
pub fn parse_srt(content: &str) -> Vec<SubtitleCue> {
    let mut cues = Vec::new();
    let blocks = content.split("\n\n");

    for block in blocks {
        let lines: Vec<&str> = block.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        if lines.len() < 2 {
            continue;
        }

        // Line 0 is usually the index number (e.g. "1") or timestamp line
        let time_line_idx = if lines[0].contains("-->") { 0 } else { 1 };
        if time_line_idx >= lines.len() {
            continue;
        }

        let time_line = lines[time_line_idx];
        if let Some((start_s, end_s)) = time_line.split_once("-->")
            && let (Some(start), Some(end)) =
                (parse_timestamp(start_s.trim()), parse_timestamp(end_s.trim()))
        {
            let text = lines[time_line_idx + 1..].join("\n");
            cues.push(SubtitleCue { start_ms: start, end_ms: end, text });
        }
    }

    cues
}

/// Parse SRT / VTT timestamp ("00:01:23,456" or "00:01:23.456") to milliseconds.
pub fn parse_timestamp(s: &str) -> Option<i64> {
    let s = s.split_whitespace().next()?; // strip extra VTT attributes
    let (hms, ms_str) = s.split_once(['.', ','])?;
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() < 2 {
        return None;
    }

    let ms: i64 = ms_str.parse().ok()?;
    let secs: i64 = parts.last()?.parse().ok()?;
    let mins: i64 = parts[parts.len() - 2].parse().ok()?;
    let hours: i64 = if parts.len() >= 3 { parts[0].parse().ok()? } else { 0 };

    Some(hours * 3_600_000 + mins * 60_000 + secs * 1_000 + ms)
}

/// Format milliseconds to SRT timestamp ("00:01:23,456").
pub fn format_timestamp(ms: i64) -> String {
    let total_ms = ms.max(0);
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s = (total_ms % 60_000) / 1_000;
    let remainder = total_ms % 1_000;
    format!("{h:02}:{m:02}:{s:02},{remainder:03}")
}

/// Serialize cues back to an SRT file string.
pub fn serialize_srt(cues: &[SubtitleCue]) -> String {
    let mut out = String::new();
    for (idx, cue) in cues.iter().enumerate() {
        out.push_str(&format!("{}\n", idx + 1));
        out.push_str(&format!(
            "{} --> {}\n",
            format_timestamp(cue.start_ms),
            format_timestamp(cue.end_ms)
        ));
        out.push_str(&cue.text);
        out.push_str("\n\n");
    }
    out
}

/// Calculate the cross-correlation alignment offset between speech segments and subtitle cues.
/// Returns the offset in milliseconds to be ADDED to subtitle cues.
pub fn find_alignment_offset(
    speech: &[SpeechSegment],
    cues: &[SubtitleCue],
    search_range_ms: i64,
    step_ms: i64,
) -> i64 {
    if speech.is_empty() || cues.is_empty() {
        return 0;
    }

    let mut best_score = -1.0;
    let mut best_offset = 0;

    let mut offset = -search_range_ms;
    while offset <= search_range_ms {
        let mut score = 0.0;
        for cue in cues {
            let shifted_start = cue.start_ms + offset;
            let shifted_end = cue.end_ms + offset;

            for sp in speech {
                let overlap_start = shifted_start.max(sp.start_ms);
                let overlap_end = shifted_end.min(sp.end_ms);
                if overlap_end > overlap_start {
                    score += (overlap_end - overlap_start) as f64;
                }
            }
        }

        if score > best_score {
            best_score = score;
            best_offset = offset;
        }

        offset += step_ms;
    }

    best_offset
}

/// Apply offset to an SRT subtitle string.
pub fn align_srt_content(content: &str, speech: &[SpeechSegment]) -> (String, i64) {
    let cues = parse_srt(content);
    let offset = find_alignment_offset(speech, &cues, 15_000, 50);

    let aligned_cues: Vec<SubtitleCue> = cues
        .into_iter()
        .map(|mut c| {
            c.start_ms = (c.start_ms + offset).max(0);
            c.end_ms = (c.end_ms + offset).max(0);
            c
        })
        .collect();

    (serialize_srt(&aligned_cues), offset)
}

/// Length of one frame of a speech map.
pub const FRAME_MS: i64 = 20;
/// Sample rate the audio is decoded to for speech detection.
pub const SPEECH_SAMPLE_RATE: u32 = 8_000;
const FRAME_SAMPLES: usize = (SPEECH_SAMPLE_RATE as i64 * FRAME_MS / 1000) as usize;

/// Finds where people speak in a film's audio, fed in chunks of 8 kHz mono 16-bit PCM (FFmpeg
/// band-limits it to the voice range first, which takes out most bass from music and effects).
/// A frame counts as speech when it is clearly louder than the quiet parts of the same film.
#[derive(Debug, Default)]
pub struct SpeechDetector {
    energies: Vec<f32>,
    sum: f64,
    count: usize,
    odd: Option<u8>,
}

impl SpeechDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Little-endian 16-bit samples, split anywhere.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut bytes = bytes;
        if let Some(low) = self.odd.take() {
            let Some((&high, rest)) = bytes.split_first() else {
                self.odd = Some(low);
                return;
            };
            self.sample(i16::from_le_bytes([low, high]));
            bytes = rest;
        }
        let (pairs, rest) = bytes.as_chunks::<2>();
        for pair in pairs {
            self.sample(i16::from_le_bytes(*pair));
        }
        self.odd = rest.first().copied();
    }

    fn sample(&mut self, value: i16) {
        let v = f64::from(value) / 32768.0;
        self.sum += v * v;
        self.count += 1;
        if self.count == FRAME_SAMPLES {
            self.energies.push((self.sum / FRAME_SAMPLES as f64 + 1e-10).log10() as f32);
            self.sum = 0.0;
            self.count = 0;
        }
    }

    /// One flag per [`FRAME_MS`] frame: is someone speaking?
    pub fn finish(self) -> Vec<bool> {
        speech_frames(&self.energies)
    }
}

/// Speech flags from per-frame log energies: smoothed over 100 ms, louder than the film's quiet
/// level by a third of the way to its loud level, and kept only in stretches of 200 ms or more.
fn speech_frames(energies: &[f32]) -> Vec<bool> {
    if energies.is_empty() {
        return Vec::new();
    }
    let smoothed: Vec<f32> = (0..energies.len())
        .map(|i| {
            let window = &energies[i.saturating_sub(2)..(i + 3).min(energies.len())];
            window.iter().sum::<f32>() / window.len() as f32
        })
        .collect();
    let mut sorted = smoothed.clone();
    sorted.sort_by(f32::total_cmp);
    let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q) as usize];
    let (quiet, loud) = (at(0.2), at(0.9));
    let threshold = quiet + (loud - quiet) / 3.0;
    let mut speech: Vec<bool> = smoothed.iter().map(|e| *e > threshold).collect();
    // Drop clicks and bangs shorter than 200 ms.
    let shortest = (200 / FRAME_MS) as usize;
    let mut i = 0;
    while i < speech.len() {
        if !speech[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < speech.len() && speech[i] {
            i += 1;
        }
        if i - start < shortest {
            speech[start..i].fill(false);
        }
    }
    speech
}

/// Speech flags packed eight to a byte, for the cache.
pub fn pack_speech(speech: &[bool]) -> Vec<u8> {
    let mut out = (speech.len() as u32).to_le_bytes().to_vec();
    for chunk in speech.chunks(8) {
        out.push(chunk.iter().enumerate().fold(0u8, |b, (i, on)| b | (u8::from(*on) << i)));
    }
    out
}

/// The reverse of [`pack_speech`]; `None` for damaged data.
pub fn unpack_speech(bytes: &[u8]) -> Option<Vec<bool>> {
    let (len, bits) = bytes.split_first_chunk::<4>()?;
    let len = u32::from_le_bytes(*len) as usize;
    if bits.len() != len.div_ceil(8) {
        return None;
    }
    Some((0..len).map(|i| bits[i / 8] & (1 << (i % 8)) != 0).collect())
}

/// How subtitles line up with the speech: shown at `time * scale + offset_ms`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alignment {
    pub offset_ms: i64,
    pub scale: f64,
    /// Share of the subtitles' time that falls on speech after the shift (0–1).
    pub score: f64,
    /// The same share without any shift.
    pub unshifted_score: f64,
}

impl Alignment {
    /// Whether the shift is worth applying: clearly better than leaving the subtitles alone.
    pub fn is_improvement(&self) -> bool {
        (self.offset_ms != 0 || self.scale != 1.0)
            && self.score >= 0.25
            && self.score - self.unshifted_score >= 0.05
    }

    pub fn apply(&self, ms: i64) -> i64 {
        ((ms as f64 * self.scale).round() as i64 + self.offset_ms).max(0)
    }
}

/// Speeds a subtitle file may have been timed for compared with the video: the same, or
/// 23.976/24/25 fps against each other (a common reason subtitles drift apart over a film).
const SCALES: [f64; 5] = [1.0, 25.0 / 23.976, 23.976 / 25.0, 24.0 / 23.976, 23.976 / 24.0];

/// The shift (up to `max_offset_ms` either way) and speed that put most of the subtitles' time
/// on speech. `cues` are (start, end) in milliseconds. `None` without speech or cues.
pub fn align_to_speech(
    speech: &[bool],
    cues: &[(i64, i64)],
    max_offset_ms: i64,
) -> Option<Alignment> {
    let total: i64 = cues.iter().map(|(a, b)| (b - a).max(0)).sum();
    if speech.iter().all(|s| !s) || total == 0 {
        return None;
    }
    // Speech frames before each frame, so a cue's overlap is two lookups.
    let mut before = vec![0u32; speech.len() + 1];
    for (i, s) in speech.iter().enumerate() {
        before[i + 1] = before[i] + u32::from(*s);
    }
    let frame = |ms: i64| (ms.max(0) / FRAME_MS).min(speech.len() as i64) as usize;
    let covered = |scale: f64, offset: i64| -> f64 {
        let frames: u32 = cues
            .iter()
            .map(|&(a, b)| {
                let start = frame((a as f64 * scale) as i64 + offset);
                let end = frame((b as f64 * scale) as i64 + offset);
                before[end.max(start)] - before[start]
            })
            .sum();
        f64::from(frames) * FRAME_MS as f64 / total as f64
    };
    let unshifted_score = covered(1.0, 0);
    let mut best = Alignment { offset_ms: 0, scale: 1.0, score: unshifted_score, unshifted_score };
    for scale in SCALES {
        let mut offset = -max_offset_ms;
        while offset <= max_offset_ms {
            let score = covered(scale, offset);
            if score > best.score + 1e-9 {
                best = Alignment { offset_ms: offset, scale, score, unshifted_score };
            }
            offset += FRAME_MS;
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 8 kHz PCM: quiet hiss, with loud stretches where `speech` (ms ranges) says so.
    fn pcm(length_ms: i64, speech: &[(i64, i64)]) -> Vec<u8> {
        let samples = length_ms * 8;
        (0..samples)
            .flat_map(|i| {
                let ms = i / 8;
                let loud = speech.iter().any(|(a, b)| (*a..*b).contains(&ms));
                let amplitude = if loud { 8000.0 } else { 150.0 };
                let v = (amplitude * ((i as f64) * 0.37).sin()) as i16;
                v.to_le_bytes()
            })
            .collect()
    }

    #[test]
    fn speech_is_found_and_late_subtitles_are_pulled_back() {
        // Speech in a 2-minute film; subtitles 2.5 s late.
        let spoken: Vec<(i64, i64)> = (0..20)
            .map(|i| (5_000 + i * 5_500, 5_000 + i * 5_500 + 2_000 + (i % 3) * 700))
            .collect();
        let mut detector = SpeechDetector::new();
        // Odd-sized chunks, as a pipe delivers them.
        for chunk in pcm(120_000, &spoken).chunks(4_097) {
            detector.feed(chunk);
        }
        let speech = detector.finish();
        assert_eq!(speech.len(), 6_000);
        assert!(speech[(6_000 / FRAME_MS) as usize]);
        assert!(!speech[(4_000 / FRAME_MS) as usize]);
        assert_eq!(unpack_speech(&pack_speech(&speech)).unwrap(), speech);

        let late: Vec<(i64, i64)> = spoken.iter().map(|(a, b)| (a + 2_500, b + 2_500)).collect();
        let found = align_to_speech(&speech, &late, 15_000).unwrap();
        assert!((-2_600..=-2_400).contains(&found.offset_ms), "{found:?}");
        assert_eq!(found.scale, 1.0);
        assert!(found.is_improvement(), "{found:?}");
        assert!(found.score > 0.9, "{found:?}");

        // Subtitles that already fit are left alone.
        let right = align_to_speech(&speech, &spoken, 15_000).unwrap();
        assert!(!right.is_improvement(), "{right:?}");
    }

    #[test]
    fn subtitles_timed_for_another_frame_rate_are_stretched() {
        // Subtitles made for 25 fps on a 23.976 fps video: they run fast and drift.
        let spoken: Vec<(i64, i64)> =
            (0..60).map(|i| (3_000 + i * 9_000, 3_000 + i * 9_000 + 2_500)).collect();
        let mut detector = SpeechDetector::new();
        detector.feed(&pcm(560_000, &spoken));
        let speech = detector.finish();
        let fast: Vec<(i64, i64)> = spoken
            .iter()
            .map(|(a, b)| ((*a as f64 * 23.976 / 25.0) as i64, (*b as f64 * 23.976 / 25.0) as i64))
            .collect();
        let found = align_to_speech(&speech, &fast, 5_000).unwrap();
        assert!((found.scale - 25.0 / 23.976).abs() < 1e-9, "{found:?}");
        assert!(found.offset_ms.abs() <= 100, "{found:?}");
        assert!(found.is_improvement());
        assert_eq!(found.apply(1_000), 1_043);
    }

    #[test]
    fn align_synthetically_shifted_subtitle() {
        // Audio speech intervals at: 3000..5000 ms and 8000..10000 ms
        let speech = vec![
            SpeechSegment { start_ms: 3_000, end_ms: 5_000 },
            SpeechSegment { start_ms: 8_000, end_ms: 10_000 },
        ];

        // Synthetically shifted subtitles (+2.8 seconds = +2800 ms late):
        // cues are at 5800..7800 ms and 10800..12800 ms
        let srt = r#"1
00:00:05,800 --> 00:00:07,800
Hello world

2
00:00:10,800 --> 00:00:12,800
Welcome to MBL
"#;

        let (aligned_srt, offset) = align_srt_content(srt, &speech);

        // Best offset to apply should be around -2800 ms (within ±100 ms)
        assert!((-2900..=-2700).contains(&offset), "expected offset ~ -2800 ms, got {offset}");

        let parsed = parse_srt(&aligned_srt);
        assert_eq!(parsed.len(), 2);
        // The first cue should now start within [2900..3100] ms
        assert!(
            (2_900..=3_100).contains(&parsed[0].start_ms),
            "aligned cue start should be near 3000 ms, got {}",
            parsed[0].start_ms
        );
    }
}
