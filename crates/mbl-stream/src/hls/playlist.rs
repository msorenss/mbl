//! Upfront HLS master and media playlist generation (`master.m3u8`, `index.m3u8`).

use std::fmt::Write;

/// Metadata for a single HLS rendition entry in `master.m3u8`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsRendition {
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bandwidth: u64,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}

/// Compute per-segment durations in milliseconds for an upfront VOD playlist.
///
/// When `segment_secs >= 6`, the first two segments use 3 s (`3000 ms`) for fast startup
/// latency, and subsequent segments use `segment_secs * 1000 ms` (M2.5).
pub fn segment_durations_ms(duration_ms: u64, segment_secs: u32) -> Vec<u64> {
    let seg_ms = u64::from(segment_secs.max(1)) * 1000;
    if duration_ms == 0 {
        return vec![seg_ms];
    }

    let first_two_ms = if segment_secs >= 6 { 3_000 } else { seg_ms };
    let mut remaining = duration_ms;
    let mut out = Vec::new();
    let mut idx = 0usize;

    while remaining > 0 {
        let target = if idx < 2 { first_two_ms } else { seg_ms };
        let chunk = remaining.min(target);
        out.push(chunk);
        remaining -= chunk;
        idx += 1;
    }
    out
}

/// Compute the start timestamp in milliseconds for `segment_index`.
pub fn segment_start_ms(segment_index: u32, segment_secs: u32) -> u64 {
    let seg_ms = u64::from(segment_secs.max(1)) * 1000;
    if segment_secs >= 6 {
        match segment_index {
            0 => 0,
            1 => 3_000,
            n => 6_000 + u64::from(n - 2) * seg_ms,
        }
    } else {
        u64::from(segment_index) * seg_ms
    }
}

/// Render the upfront HLS master playlist (`master.m3u8`).
pub fn render_master_playlist(renditions: &[HlsRendition]) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("#EXTM3U\n");
    out.push_str("#EXT-X-VERSION:7\n");
    out.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");

    for r in renditions {
        let _ = write!(out, "#EXT-X-STREAM-INF:BANDWIDTH={}", r.bandwidth.max(128_000));
        if let (Some(w), Some(h)) = (r.width, r.height) {
            let _ = write!(out, ",RESOLUTION={w}x{h}");
        }
        let codecs = rfc6381_codecs(r.video_codec.as_deref(), r.audio_codec.as_deref());
        if !codecs.is_empty() {
            let _ = write!(out, ",CODECS=\"{codecs}\"");
        }
        out.push('\n');
        let _ = writeln!(out, "{}/index.m3u8", r.name);
    }
    out
}

/// Render the upfront HLS VOD media playlist (`index.m3u8`) from the total media duration.
pub fn render_media_playlist(duration_ms: u64, segment_secs: u32) -> String {
    let target_duration = segment_secs.max(1);
    let durations = segment_durations_ms(duration_ms, target_duration);

    let mut out = String::with_capacity(128 + durations.len() * 28);
    out.push_str("#EXTM3U\n");
    out.push_str("#EXT-X-VERSION:7\n");
    let _ = writeln!(out, "#EXT-X-TARGETDURATION:{target_duration}");
    out.push_str("#EXT-X-MEDIA-SEQUENCE:0\n");
    out.push_str("#EXT-X-PLAYLIST-TYPE:VOD\n");
    out.push_str("#EXT-X-MAP:URI=\"init.mp4\"\n");

    for (idx, dur_ms) in durations.iter().enumerate() {
        let secs = *dur_ms / 1000;
        let millis = *dur_ms % 1000;
        let _ = writeln!(out, "#EXTINF:{secs}.{millis:03},");
        let _ = writeln!(out, "{idx}.m4s");
    }

    out.push_str("#EXT-X-ENDLIST\n");
    out
}

fn rfc6381_codecs(video: Option<&str>, audio: Option<&str>) -> String {
    let mut parts = Vec::with_capacity(2);
    if let Some(v) = video {
        let lower = v.to_ascii_lowercase();
        let mapped = match lower.as_str() {
            "h264" | "avc" | "libx264" => "avc1.640028",
            "hevc" | "h265" | "libx265" => "hvc1.1.6.L120.90",
            "av1" | "libsvtav1" => "av01.0.08M.08",
            other => other,
        };
        parts.push(mapped.to_owned());
    }
    if let Some(a) = audio {
        let lower = a.to_ascii_lowercase();
        let mapped = match lower.as_str() {
            "aac" => "mp4a.40.2",
            "ac3" => "ac-3",
            "eac3" => "ec-3",
            "opus" | "libopus" => "Opus",
            "flac" => "fLaC",
            "mp3" => "mp4a.40.34",
            other => other,
        };
        parts.push(mapped.to_owned());
    }
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_playlist_uses_3s_for_first_two_segments_then_6s() {
        // 24s total: 3s + 3s + 6s + 6s + 6s = 5 segments (0..4)
        let durations = segment_durations_ms(24_000, 6);
        assert_eq!(durations, vec![3_000, 3_000, 6_000, 6_000, 6_000]);
        assert_eq!(segment_start_ms(0, 6), 0);
        assert_eq!(segment_start_ms(1, 6), 3_000);
        assert_eq!(segment_start_ms(2, 6), 6_000);
        assert_eq!(segment_start_ms(3, 6), 12_000);
        assert_eq!(segment_start_ms(4, 6), 18_000);

        let m3u8 = render_media_playlist(24_000, 6);
        assert!(m3u8.contains("#EXT-X-TARGETDURATION:6\n"));
        assert!(m3u8.contains("#EXT-X-PLAYLIST-TYPE:VOD\n"));
        assert!(m3u8.contains("#EXT-X-MAP:URI=\"init.mp4\"\n"));
        assert!(
            m3u8.contains("#EXTINF:3.000,\n0.m4s\n#EXTINF:3.000,\n1.m4s\n#EXTINF:6.000,\n2.m4s\n")
        );
        assert!(m3u8.ends_with("#EXT-X-ENDLIST\n"));
    }

    #[test]
    fn master_playlist_renders_stream_inf_and_rfc6381_codecs() {
        let master = render_master_playlist(&[HlsRendition {
            name: "0".to_owned(),
            width: Some(1920),
            height: Some(1080),
            bandwidth: 8_256_000,
            video_codec: Some("h264".to_owned()),
            audio_codec: Some("aac".to_owned()),
        }]);
        assert!(master.contains("#EXTM3U\n"));
        assert!(master.contains(
            "#EXT-X-STREAM-INF:BANDWIDTH=8256000,RESOLUTION=1920x1080,CODECS=\"avc1.640028,mp4a.40.2\"\n0/index.m3u8\n"
        ));
    }
}
