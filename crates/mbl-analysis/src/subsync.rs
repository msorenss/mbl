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

#[cfg(test)]
mod tests {
    use super::*;

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
