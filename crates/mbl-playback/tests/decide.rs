#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use mbl_core::StreamKind;
use mbl_playback::{
    BUILTIN_PROFILES, CodecLimit, DecisionContext, DeviceProfile, DeviceProfileOverlay, HwSummary,
    Network, PlayMethod, Reason, StreamAction, SubtitleDelivery, decide,
};
use mbl_probe::{HdrKind, MediaInfo, StreamInfo, parse_ffprobe_json};

fn probe_fixture(name: &str) -> MediaInfo {
    let path = format!("{}/../mbl-probe/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap();
    parse_ffprobe_json(&bytes).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn synthetic_video_media(
    container: &str,
    video_codec: &str,
    profile: Option<&str>,
    level: Option<i32>,
    width: u32,
    height: u32,
    bit_depth: u8,
    hdr: Option<HdrKind>,
    interlaced: bool,
    audio_codec: &str,
    channels: u8,
    bitrate: Option<u64>,
) -> MediaInfo {
    MediaInfo {
        container: container.into(),
        duration_ms: 60_000,
        bitrate,
        size: bitrate.unwrap_or(4_000_000) * 60 / 8,
        streams: vec![
            StreamInfo {
                index: 0,
                kind: StreamKind::Video,
                codec: video_codec.into(),
                profile: profile.map(str::to_owned),
                level,
                language: Some("eng".into()),
                title: None,
                default: true,
                forced: false,
                external_path: None,
                width: Some(width),
                height: Some(height),
                bit_depth: Some(bit_depth),
                frame_rate: Some(24.0),
                interlaced,
                hdr,
                channels: None,
                channel_layout: None,
                sample_rate: None,
            },
            StreamInfo {
                index: 1,
                kind: StreamKind::Audio,
                codec: audio_codec.into(),
                profile: None,
                level: None,
                language: Some("eng".into()),
                title: None,
                default: true,
                forced: false,
                external_path: None,
                width: None,
                height: None,
                bit_depth: None,
                frame_rate: None,
                interlaced: false,
                hdr: None,
                channels: Some(channels),
                channel_layout: None,
                sample_rate: Some(48_000),
            },
        ],
        chapters: vec![],
        tags: BTreeMap::new(),
    }
}

#[derive(Debug)]
struct MatrixCase {
    fixture: &'static str,
    profile: &'static str,
    method: PlayMethod,
    video_copied: bool,
    audio_copied: bool,
    expected_reasons: &'static [&'static str],
}

#[test]
fn fixture_profile_matrix_55_cases() {
    let cases = [
        // 1. h264_aac_mp4 (720p H.264 8-bit + AAC stereo in MP4) -> DirectPlay everywhere
        MatrixCase {
            fixture: "h264_aac_mp4",
            profile: "chrome",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "h264_aac_mp4",
            profile: "firefox",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "h264_aac_mp4",
            profile: "safari",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "h264_aac_mp4",
            profile: "edge",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "h264_aac_mp4",
            profile: "generic-hls",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        // 2. h264_ac3_subs_mkv (720p H.264 + AC3 5.1 in MKV)
        // Chrome/Firefox/generic-hls: copy video, transcode audio to AAC
        // Safari/Edge: DirectStream (remux both H.264 and AC3 into fMP4)
        MatrixCase {
            fixture: "h264_ac3_subs_mkv",
            profile: "chrome",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported"],
        },
        MatrixCase {
            fixture: "h264_ac3_subs_mkv",
            profile: "firefox",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported"],
        },
        MatrixCase {
            fixture: "h264_ac3_subs_mkv",
            profile: "safari",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "h264_ac3_subs_mkv",
            profile: "edge",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "h264_ac3_subs_mkv",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported", "tooManyChannels"],
        },
        // 3. hevc_hdr10_mkv (720p HEVC Main 10 HDR10 + AAC stereo in MKV)
        // Chrome/Safari/Edge: DirectStream (remux to fMP4)
        // Firefox/generic-hls: Transcode video to H.264, copy audio
        MatrixCase {
            fixture: "hevc_hdr10_mkv",
            profile: "chrome",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_hdr10_mkv",
            profile: "firefox",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &["videoCodecUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_hdr10_mkv",
            profile: "safari",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_hdr10_mkv",
            profile: "edge",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_hdr10_mkv",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &["containerUnsupported", "videoCodecUnsupported"],
        },
        // 4. hevc_dts_mkv (720p HEVC Main 8-bit + DTS 5.1 in MKV)
        MatrixCase {
            fixture: "hevc_dts_mkv",
            profile: "chrome",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_dts_mkv",
            profile: "firefox",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &["videoCodecUnsupported", "audioCodecUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_dts_mkv",
            profile: "safari",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_dts_mkv",
            profile: "edge",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported"],
        },
        MatrixCase {
            fixture: "hevc_dts_mkv",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "containerUnsupported",
                "videoCodecUnsupported",
                "audioCodecUnsupported",
                "tooManyChannels",
            ],
        },
        // 5. av1_opus_mkv (720p AV1 + Opus in Matroska/WebM)
        MatrixCase {
            fixture: "av1_opus_mkv",
            profile: "chrome",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "av1_opus_mkv",
            profile: "firefox",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "av1_opus_mkv",
            profile: "safari",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported"],
        },
        MatrixCase {
            fixture: "av1_opus_mkv",
            profile: "edge",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "av1_opus_mkv",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "containerUnsupported",
                "videoCodecUnsupported",
                "audioCodecUnsupported",
            ],
        },
        // 6. vp9_opus_webm (720p VP9 + Opus in WebM)
        MatrixCase {
            fixture: "vp9_opus_webm",
            profile: "chrome",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "vp9_opus_webm",
            profile: "firefox",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "vp9_opus_webm",
            profile: "safari",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "vp9_opus_webm",
            profile: "edge",
            method: PlayMethod::DirectPlay,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "vp9_opus_webm",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "containerUnsupported",
                "videoCodecUnsupported",
                "audioCodecUnsupported",
            ],
        },
        // 7. multi_audio_mkv (720p H.264 + 2x AAC stereo in MKV) -> DirectStream everywhere
        MatrixCase {
            fixture: "multi_audio_mkv",
            profile: "chrome",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "multi_audio_mkv",
            profile: "firefox",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "multi_audio_mkv",
            profile: "safari",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "multi_audio_mkv",
            profile: "edge",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        MatrixCase {
            fixture: "multi_audio_mkv",
            profile: "generic-hls",
            method: PlayMethod::DirectStream,
            video_copied: true,
            audio_copied: true,
            expected_reasons: &["containerUnsupported"],
        },
        // 8. mpeg2_interlaced_ts (576i MPEG-2 + AC3 stereo in MPEG-TS)
        MatrixCase {
            fixture: "mpeg2_interlaced_ts",
            profile: "chrome",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "containerUnsupported",
                "videoCodecUnsupported",
                "other",
                "audioCodecUnsupported",
            ],
        },
        MatrixCase {
            fixture: "mpeg2_interlaced_ts",
            profile: "firefox",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "containerUnsupported",
                "videoCodecUnsupported",
                "other",
                "audioCodecUnsupported",
            ],
        },
        MatrixCase {
            fixture: "mpeg2_interlaced_ts",
            profile: "safari",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &["containerUnsupported", "videoCodecUnsupported", "other"],
        },
        MatrixCase {
            fixture: "mpeg2_interlaced_ts",
            profile: "edge",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &["containerUnsupported", "videoCodecUnsupported", "other"],
        },
        MatrixCase {
            fixture: "mpeg2_interlaced_ts",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "containerUnsupported",
                "videoCodecUnsupported",
                "other",
                "audioCodecUnsupported",
            ],
        },
        // 9. flac_tagged (FLAC audio)
        MatrixCase {
            fixture: "flac_tagged",
            profile: "chrome",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "flac_tagged",
            profile: "firefox",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "flac_tagged",
            profile: "safari",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "flac_tagged",
            profile: "edge",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "flac_tagged",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported"],
        },
        // 10. mp3_tagged (MP3 audio) -> DirectPlay everywhere
        MatrixCase {
            fixture: "mp3_tagged",
            profile: "chrome",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "mp3_tagged",
            profile: "firefox",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "mp3_tagged",
            profile: "safari",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "mp3_tagged",
            profile: "edge",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        MatrixCase {
            fixture: "mp3_tagged",
            profile: "generic-hls",
            method: PlayMethod::DirectPlay,
            video_copied: false,
            audio_copied: true,
            expected_reasons: &[],
        },
        // 11. dv8_truehd_pgs_chapters (4K HEVC DV8 compat_id=1 + TrueHD 7.1 in MKV)
        // Chrome/Safari/Edge: DV8 falls back to HDR10 (or native DV on Safari), so video copies, TrueHD 8ch transcodes
        // Firefox/generic-hls: video and audio both transcode
        MatrixCase {
            fixture: "dv8_truehd_pgs_chapters",
            profile: "chrome",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported", "tooManyChannels"],
        },
        MatrixCase {
            fixture: "dv8_truehd_pgs_chapters",
            profile: "firefox",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "videoCodecUnsupported",
                "audioCodecUnsupported",
                "tooManyChannels",
            ],
        },
        MatrixCase {
            fixture: "dv8_truehd_pgs_chapters",
            profile: "safari",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported", "tooManyChannels"],
        },
        MatrixCase {
            fixture: "dv8_truehd_pgs_chapters",
            profile: "edge",
            method: PlayMethod::Transcode,
            video_copied: true,
            audio_copied: false,
            expected_reasons: &["containerUnsupported", "audioCodecUnsupported", "tooManyChannels"],
        },
        MatrixCase {
            fixture: "dv8_truehd_pgs_chapters",
            profile: "generic-hls",
            method: PlayMethod::Transcode,
            video_copied: false,
            audio_copied: false,
            expected_reasons: &[
                "containerUnsupported",
                "videoCodecUnsupported",
                "audioCodecUnsupported",
                "tooManyChannels",
            ],
        },
    ];

    assert_eq!(cases.len(), 55);

    let ctx = DecisionContext::default();
    for case in &cases {
        let media = probe_fixture(case.fixture);
        let profile = DeviceProfile::builtin(case.profile).unwrap();
        let dec = decide(&media, &profile, &ctx);

        assert_eq!(dec.method, case.method, "{case:?} -> {dec:?}");
        if case.video_copied {
            assert_eq!(dec.video, StreamAction::Copy, "{case:?}");
        } else if media.primary_video().is_some() {
            assert!(matches!(dec.video, StreamAction::Transcode { .. }), "{case:?}");
        } else {
            assert_eq!(dec.video, StreamAction::Drop, "{case:?}");
        }

        if case.audio_copied {
            assert_eq!(dec.audio, StreamAction::Copy, "{case:?}");
        } else {
            assert!(matches!(dec.audio, StreamAction::Transcode { .. }), "{case:?}");
        }

        let codes: Vec<String> = dec
            .reasons
            .iter()
            .map(|r| serde_json::to_value(r).unwrap()["code"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(codes, case.expected_reasons, "{case:?}");
    }
}

#[test]
fn edge_cases_and_constraints_matrix() {
    let chrome = DeviceProfile::builtin("chrome").unwrap();
    let firefox = DeviceProfile::builtin("firefox").unwrap();
    let safari = DeviceProfile::builtin("safari").unwrap();
    let generic = DeviceProfile::builtin("generic-hls").unwrap();

    // Case 56: Text subtitle (subrip) -> Sidecar VTT without forcing video transcode
    let sub_mkv = probe_fixture("h264_ac3_subs_mkv");
    let dec = decide(
        &sub_mkv,
        &safari,
        &DecisionContext { subtitle_index: Some(2), ..Default::default() },
    );
    assert_eq!(dec.method, PlayMethod::DirectStream);
    assert_eq!(dec.video, StreamAction::Copy);
    assert_eq!(dec.subtitle, SubtitleDelivery::Sidecar { format: "vtt".into() });

    // Case 57: ASS subtitle -> Sidecar VTT by default on Chrome
    let dec = decide(
        &sub_mkv,
        &chrome,
        &DecisionContext { subtitle_index: Some(3), ..Default::default() },
    );
    assert_eq!(dec.video, StreamAction::Copy);
    assert_eq!(dec.subtitle, SubtitleDelivery::Sidecar { format: "vtt".into() });

    // Case 58: ASS subtitle with custom overlay target_format = "ass"
    let ass_chrome = DeviceProfileOverlay {
        name: "chrome".into(),
        subtitles: Some(vec![mbl_playback::SubtitleRule {
            format: "ass".into(),
            method: mbl_playback::SubtitleMethod::External,
            target_format: Some("ass".into()),
        }]),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    let dec = decide(
        &sub_mkv,
        &ass_chrome,
        &DecisionContext { subtitle_index: Some(3), ..Default::default() },
    );
    assert_eq!(dec.subtitle, SubtitleDelivery::Sidecar { format: "ass".into() });

    // Case 59: Image subtitle (hdmv_pgs_subtitle) selected -> forces video transcode with BurnIn
    let dv_mkv = probe_fixture("dv8_truehd_pgs_chapters");
    let dec = decide(
        &dv_mkv,
        &safari,
        &DecisionContext {
            audio_index: Some(2),    // select AC3 5.1 stream so audio copies on Safari
            subtitle_index: Some(3), // PGS subtitle stream
            ..Default::default()
        },
    );
    assert_eq!(dec.method, PlayMethod::Transcode);
    assert_eq!(dec.subtitle, SubtitleDelivery::BurnIn);
    assert_eq!(dec.audio, StreamAction::Copy);
    assert!(matches!(dec.video, StreamAction::Transcode { .. }));
    assert!(dec.reasons.contains(&Reason::SubtitleBurnInRequired("hdmv_pgs_subtitle".into())));

    // Case 60: Selecting alternate compatible audio track (AC3 5.1 on Safari) avoids audio transcode
    let dec =
        decide(&dv_mkv, &safari, &DecisionContext { audio_index: Some(2), ..Default::default() });
    assert_eq!(dec.method, PlayMethod::DirectStream);
    assert_eq!(dec.video, StreamAction::Copy);
    assert_eq!(
        dec.reasons,
        vec![
            Reason::Other("Selected audio track requires stream remuxing".into()),
            Reason::ContainerUnsupported("matroska,webm".into()),
        ]
    );

    // Case 61: Bitrate limit in DecisionContext forces video transcode and resolution scaling
    let mp4 = probe_fixture("h264_aac_mp4");
    let dec = decide(
        &mp4,
        &chrome,
        &DecisionContext {
            max_bitrate: Some(800_000),
            network: Network::Wan,
            ..Default::default()
        },
    );
    assert_eq!(dec.method, PlayMethod::Transcode);
    assert_eq!(
        dec.video,
        StreamAction::Transcode { codec: "h264".into(), bitrate: 800_000, width: Some(854) }
    );
    assert!(dec.reasons.iter().any(|r| matches!(r, Reason::BitrateTooHigh { .. })));

    // Case 62: Profile max_bitrate forces transcode even when ctx.max_bitrate is None
    let capped_chrome = DeviceProfileOverlay {
        name: "chrome".into(),
        max_bitrate: Some(2_000_000),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    let dec = decide(&mp4, &capped_chrome, &DecisionContext::default());
    assert_eq!(dec.method, PlayMethod::Transcode);
    assert!(dec.reasons.iter().any(|r| matches!(r, Reason::BitrateTooHigh { .. })));

    // Case 63: H.264 10-bit High 10 profile rejected on Chrome -> BitDepthUnsupported + VideoProfileUnsupported
    let hi10p = synthetic_video_media(
        "mp4",
        "h264",
        Some("High 10"),
        Some(41),
        1920,
        1080,
        10,
        None,
        false,
        "aac",
        2,
        Some(5_000_000),
    );
    let dec = decide(&hi10p, &chrome, &DecisionContext::default());
    assert_eq!(dec.method, PlayMethod::Transcode);
    assert!(dec.reasons.contains(&Reason::VideoProfileUnsupported("h264 High 10".into())));
    assert!(dec.reasons.contains(&Reason::BitDepthUnsupported(10)));

    // Case 64: H.264 level 52 rejected on generic-hls (max_level 41) and 4K resolution rejected (max 1920x1080)
    let uhd_h264 = synthetic_video_media(
        "mp4",
        "h264",
        Some("High"),
        Some(52),
        3840,
        2160,
        8,
        None,
        false,
        "aac",
        2,
        Some(15_000_000),
    );
    let dec = decide(&uhd_h264, &generic, &DecisionContext::default());
    assert_eq!(dec.method, PlayMethod::Transcode);
    assert!(dec.reasons.contains(&Reason::ResolutionTooHigh));
    assert!(
        dec.reasons
            .contains(&Reason::VideoProfileUnsupported("h264 level 52 exceeds max 41".into()))
    );
    assert_eq!(
        dec.video,
        StreamAction::Transcode { codec: "h264".into(), bitrate: 8_000_000, width: Some(1920) }
    );

    // Case 65: Dolby Vision Profile 5 (no HDR10 base layer) rejected on Chrome (which supports HDR10/HLG but not DV),
    // but DirectPlays on Safari (which supports DV)!
    let dv5_mp4 = synthetic_video_media(
        "mp4",
        "hevc",
        Some("Main 10"),
        Some(150),
        3840,
        2160,
        10,
        Some(HdrKind::DolbyVision { profile: 5, compat_id: 0 }),
        false,
        "aac",
        2,
        Some(20_000_000),
    );
    let chrome_dv5 = decide(&dv5_mp4, &chrome, &DecisionContext::default());
    assert_eq!(chrome_dv5.method, PlayMethod::Transcode);
    assert!(chrome_dv5.reasons.contains(&Reason::HdrUnsupported));

    // Case 66: DV5 on Safari -> DirectPlay
    let safari_dv5 = decide(&dv5_mp4, &safari, &DecisionContext::default());
    assert_eq!(safari_dv5.method, PlayMethod::DirectPlay);
    assert!(safari_dv5.reasons.is_empty());

    // Case 67: No transcode capacity when video transcode is required and hw is None
    let hdr_mkv = probe_fixture("hevc_hdr10_mkv");
    let no_cap = decide(
        &hdr_mkv,
        &firefox,
        &DecisionContext { can_transcode_video: false, hw: None, ..Default::default() },
    );
    assert!(no_cap.is_refused());
    assert!(no_cap.reasons.contains(&Reason::NoTranscodeCapacity));

    // Case 68: Hardware summary present allows video transcode even when SW capacity is false
    let with_hw = decide(
        &hdr_mkv,
        &firefox,
        &DecisionContext {
            can_transcode_video: false,
            hw: Some(HwSummary {
                backend: "nvenc".into(),
                encode_codecs: vec!["h264".into()],
                can_tonemap: true,
            }),
            ..Default::default()
        },
    );
    assert!(!with_hw.is_refused());

    // Case 69: H.264 + AAC in MKV -> DirectStream (remux) on Firefox with only ContainerUnsupported
    let h264_mkv = synthetic_video_media(
        "matroska,webm",
        "h264",
        Some("High"),
        Some(41),
        1920,
        1080,
        8,
        None,
        false,
        "aac",
        2,
        Some(6_000_000),
    );
    let remux = decide(&h264_mkv, &firefox, &DecisionContext::default());
    assert_eq!(remux.method, PlayMethod::DirectStream);
    assert_eq!(remux.video, StreamAction::Copy);
    assert_eq!(remux.audio, StreamAction::Copy);
    assert_eq!(remux.reasons, vec![Reason::ContainerUnsupported("matroska,webm".into())]);

    // Case 70: VP9 HDR10 on Firefox (VP9 supported, 10-bit supported, but HDR not supported) -> HdrUnsupported
    let vp9_hdr = synthetic_video_media(
        "webm",
        "vp9",
        Some("Profile 2"),
        None,
        1920,
        1080,
        10,
        Some(HdrKind::Hdr10),
        false,
        "opus",
        2,
        Some(6_000_000),
    );
    let vp9_ff = decide(&vp9_hdr, &firefox, &DecisionContext::default());
    assert_eq!(vp9_ff.method, PlayMethod::Transcode);
    assert_eq!(vp9_ff.reasons, vec![Reason::HdrUnsupported]);

    // Case 71: Custom CodecLimit overlay restricting Chrome to 720p H.264
    let mobile_chrome = DeviceProfileOverlay {
        name: "chrome".into(),
        codec_limits: Some(vec![CodecLimit {
            codec: "h264".into(),
            profiles: vec!["Main".into()],
            max_level: Some(31),
            max_width: Some(1280),
            max_height: Some(720),
            max_bit_depth: Some(8),
            hdr: false,
            hdr_formats: vec![],
        }]),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    let fhd = synthetic_video_media(
        "mp4",
        "h264",
        Some("Main"),
        Some(31),
        1920,
        1080,
        8,
        None,
        false,
        "aac",
        2,
        Some(4_000_000),
    );
    let dec = decide(&fhd, &mobile_chrome, &DecisionContext::default());
    assert_eq!(dec.method, PlayMethod::Transcode);
    assert_eq!(dec.reasons, vec![Reason::ResolutionTooHigh]);
    assert_eq!(
        dec.video,
        StreamAction::Transcode { codec: "h264".into(), bitrate: 4_000_000, width: Some(1280) }
    );

    // Case 72 (M4.10): HDR10 on Firefox when running on a Pi 5 profile (`piDrmDecode`, `can_tonemap: false`)
    // explains that software HDR tone mapping is disabled on Raspberry Pi profiles and marks refused.
    let pi_hdr = decide(
        &hdr_mkv,
        &firefox,
        &DecisionContext {
            can_transcode_video: true,
            hw: Some(HwSummary {
                backend: "piDrmDecode".into(),
                encode_codecs: vec!["h264".into()],
                can_tonemap: false,
            }),
            ..Default::default()
        },
    );
    assert!(pi_hdr.is_refused());
    assert!(pi_hdr.reasons.contains(&Reason::Other(
        "Software HDR tone mapping is disabled on Raspberry Pi profiles".into()
    )));

    // Case 73 (M4.14): Remote playback (WAN / Tailnet) prefers AV1 or HEVC when supported by both
    // the client profile and hardware encoder, whereas LAN prefers H.264.
    let remote_av1 = decide(
        &mp4,
        &chrome,
        &DecisionContext {
            max_bitrate: Some(2_000_000),
            network: Network::Wan,
            can_transcode_video: true,
            hw: Some(HwSummary {
                backend: "nvenc".into(),
                encode_codecs: vec!["h264".into(), "hevc".into(), "av1".into()],
                can_tonemap: true,
            }),
            ..Default::default()
        },
    );
    assert_eq!(
        remote_av1.video,
        StreamAction::Transcode { codec: "av1".into(), bitrate: 2_000_000, width: None }
    );

    let remote_hevc_safari = decide(
        &mp4,
        &safari,
        &DecisionContext {
            max_bitrate: Some(2_000_000),
            network: Network::TailnetDirect,
            can_transcode_video: true,
            hw: Some(HwSummary {
                backend: "vaapi".into(),
                encode_codecs: vec!["h264".into(), "hevc".into()],
                can_tonemap: true,
            }),
            ..Default::default()
        },
    );
    assert_eq!(
        remote_hevc_safari.video,
        StreamAction::Transcode { codec: "hevc".into(), bitrate: 2_000_000, width: None }
    );
}

#[test]
fn reason_messages_snapshot() {
    let all_reasons = vec![
        Reason::ContainerUnsupported("matroska,webm".into()),
        Reason::VideoCodecUnsupported("hevc".into()),
        Reason::VideoProfileUnsupported("h264 High 10".into()),
        Reason::BitDepthUnsupported(10),
        Reason::HdrUnsupported,
        Reason::ResolutionTooHigh,
        Reason::BitrateTooHigh { source: 25_000_000, limit: 8_000_000 },
        Reason::AudioCodecUnsupported("truehd".into()),
        Reason::TooManyChannels(8),
        Reason::SubtitleBurnInRequired("hdmv_pgs_subtitle".into()),
        Reason::NoTranscodeCapacity,
        Reason::Other("Video stream 'mpeg2video' is interlaced".into()),
    ];
    let formatted: Vec<String> = all_reasons.iter().map(|r| format!("{r:?} => {r}")).collect();
    insta::assert_debug_snapshot!("all_reason_messages", formatted);

    // Snapshot of fixture × profile non-direct decisions and their human messages
    let fixtures = [
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
    ];
    let ctx = DecisionContext::default();
    let mut matrix_summary = Vec::new();
    for fixture in fixtures {
        let media = probe_fixture(fixture);
        for &prof_name in BUILTIN_PROFILES {
            let profile = DeviceProfile::builtin(prof_name).unwrap();
            let dec = decide(&media, &profile, &ctx);
            matrix_summary.push(format!(
                "{fixture} @ {prof_name}: {:?} | video={:?} | audio={:?} | reasons={:?}",
                dec.method,
                dec.video,
                dec.audio,
                dec.reason_messages()
            ));
        }
    }
    insta::assert_debug_snapshot!("fixture_profile_decisions", matrix_summary);
}
#[test]
fn selecting_alternate_audio_remuxes_without_reencoding_video() {
    let mut media = synthetic_video_media(
        "mp4",
        "h264",
        Some("High"),
        Some(40),
        1920,
        1080,
        8,
        None,
        false,
        "aac",
        2,
        Some(4_000_000),
    );
    let mut alternate = media.streams[1].clone();
    alternate.index = 2;
    alternate.default = false;
    alternate.language = Some("swe".into());
    media.streams.push(alternate);
    let profile = DeviceProfile::builtin("chrome").unwrap();
    let ctx = DecisionContext { audio_index: Some(2), ..Default::default() };
    let decision = decide(&media, &profile, &ctx);
    assert_eq!(decision.method, PlayMethod::DirectStream);
    assert_eq!(decision.video, StreamAction::Copy);
    assert_eq!(decision.audio, StreamAction::Copy);
}
