#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use mbl_core::StreamKind;
use mbl_probe::{FfTools, HdrKind, parse_ffprobe_json};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn snapshots() {
    for name in [
        "h264_aac_mp4",
        "h264_ac3_subs_mkv",
        "hevc_hdr10_mkv",
        "hevc_dts_mkv",
        "av1_opus_mkv",
        "vp9_opus_webm",
        "multi_audio_mkv",
        "mpeg2_interlaced_ts",
        "flac_tagged",
        "mp3_tagged",
        "dv8_truehd_pgs_chapters",
    ] {
        let info = parse_ffprobe_json(&fixture(name)).unwrap();
        insta::assert_debug_snapshot!(name, info);
    }
}

#[test]
fn hdr_and_interlace_detection() {
    let hdr10 = parse_ffprobe_json(&fixture("hevc_hdr10_mkv")).unwrap();
    let v = hdr10.primary_video().unwrap();
    assert_eq!(v.hdr, Some(HdrKind::Hdr10));
    assert_eq!(v.bit_depth, Some(10));

    let ts = parse_ffprobe_json(&fixture("mpeg2_interlaced_ts")).unwrap();
    assert!(ts.primary_video().unwrap().interlaced);

    let dv = parse_ffprobe_json(&fixture("dv8_truehd_pgs_chapters")).unwrap();
    assert_eq!(
        dv.primary_video().unwrap().hdr,
        Some(HdrKind::DolbyVision { profile: 8, compat_id: 1 })
    );
    assert_eq!(dv.streams[dv.streams.len() - 1].hdr, Some(HdrKind::Hdr10Plus));
    assert_eq!(dv.chapters.len(), 3);
    assert_eq!(dv.chapters[2].end_ms, 7_380_500);
    assert!(dv.streams.iter().all(|s| s.codec != "bin_data"), "data streams are dropped");
    let forced = dv.streams_of(StreamKind::Subtitle).find(|s| s.forced).unwrap();
    assert_eq!(forced.codec, "hdmv_pgs_subtitle");
    let und = dv.streams_of(StreamKind::Subtitle).find(|s| s.codec == "subrip").unwrap();
    assert_eq!(und.language, None, "'und' language is normalized to None");
}

#[test]
fn multi_audio_languages() {
    let info = parse_ffprobe_json(&fixture("multi_audio_mkv")).unwrap();
    let langs: Vec<_> = info.streams_of(StreamKind::Audio).map(|s| s.language.clone()).collect();
    assert_eq!(langs, [Some("eng".into()), Some("ger".into())]);
}

/// Probes the generated library when `cargo xtask gen-media` has been run and FFmpeg is present.
#[tokio::test]
async fn probes_generated_library() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/out");
    let Ok(manifest) = std::fs::read(root.join("manifest.json")) else {
        eprintln!("skipping: run `cargo xtask gen-media --quick` first");
        return;
    };
    let Ok(tools) = FfTools::locate("", "").await else {
        eprintln!("skipping: ffprobe not found");
        return;
    };
    let manifest: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    for file in manifest["files"].as_array().unwrap() {
        let path = root.join(file["path"].as_str().unwrap());
        let info = tools.probe(&path, mbl_probe::DEFAULT_PROBE_TIMEOUT).await.unwrap();
        let expected = &file["expected"];
        assert_eq!(info.container, expected["container"].as_str().unwrap(), "{path:?}");
        let video = info.primary_video().map(|v| v.codec.clone());
        assert_eq!(video.as_deref(), expected["videoCodec"].as_str(), "{path:?}");
        if let Some(v) = info.primary_video() {
            assert_eq!(v.hdr.is_some(), expected["hdr"].as_bool().unwrap(), "{path:?}");
            assert_eq!(v.interlaced, expected["interlaced"].as_bool().unwrap(), "{path:?}");
        }
        assert!(info.duration_ms > 0, "{path:?}");
    }
}

#[tokio::test]
async fn unreadable_file_is_a_validation_error() {
    let Ok(tools) = FfTools::locate("", "").await else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("mbl-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bogus = dir.join("concat:not-media.mkv");
    std::fs::write(&bogus, b"this is not a video").unwrap();
    let err = tools.probe(&bogus, mbl_probe::DEFAULT_PROBE_TIMEOUT).await.unwrap_err();
    assert!(matches!(err, mbl_core::Error::Validation(_)), "{err:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn missing_binary_is_unavailable() {
    let err = FfTools::locate("/nonexistent/ffmpeg", "/nonexistent/ffprobe").await.unwrap_err();
    assert!(matches!(err, mbl_core::Error::Unavailable(_)));
}
