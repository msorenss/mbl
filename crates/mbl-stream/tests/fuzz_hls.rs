//! Fuzzing and robustness test suite for HLS and Range parsing (M10.2).

use mbl_stream::direct::parse_range_header;
use mbl_stream::hls::parse_segment_filename;
use mbl_stream::hls::playlist::{HlsRendition, render_master_playlist, render_media_playlist};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn parse_segment_filename_never_panics(name in "\\PC{0,100}") {
        let _ = parse_segment_filename(&name);
    }

    #[test]
    fn parse_range_header_never_panics(header in "\\PC{0,200}", file_size in any::<u64>()) {
        let _ = parse_range_header(&header, file_size);
    }

    #[test]
    fn render_media_playlist_never_panics(
        total_duration_ms in 0u64..10_000_000,
        segment_secs in 1u32..60,
        session_id in "\\PC{0,50}",
    ) {
        let _ = render_media_playlist(total_duration_ms, segment_secs, &session_id, None);
    }

    #[test]
    fn render_master_playlist_never_panics(
        rendition_count in 0usize..5,
        bitrate in any::<u32>(),
        width in any::<u32>(),
        height in any::<u32>(),
    ) {
        let renditions: Vec<_> = (0..rendition_count)
            .map(|i| HlsRendition {
                name: format!("rend_{i}"),
                bitrate,
                width,
                height,
                codecs: "avc1.640028,mp4a.40.2".into(),
            })
            .collect();
        let _ = render_master_playlist(&renditions, "sess-1");
    }
}
