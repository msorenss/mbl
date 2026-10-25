#![allow(clippy::unwrap_used)]

use std::process::Stdio;

use camino::Utf8PathBuf;
use mbl_core::StreamKind;
use mbl_transcode::{
    AudioCodec, AudioOp, Backend, FfmpegCaps, HlsOutput, Pipeline, Rendition, SubtitleOp,
    VideoCodec, VideoOp, build_args,
};

fn default_output() -> HlsOutput {
    HlsOutput {
        dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-01"),
        segment_secs: 6,
        start_number: 0,
        renditions: vec![Rendition {
            name: "0".to_owned(),
            width: Some(1920),
            max_bitrate: 8_000_000,
        }],
    }
}

fn args_to_strings(p: &Pipeline, caps: &FfmpegCaps) -> Vec<String> {
    build_args(p, caps).unwrap().into_iter().map(|s| s.into_string().unwrap()).collect()
}

struct PipelineCase {
    name: &'static str,
    pipeline: Pipeline,
    caps: FfmpegCaps,
}

fn twenty_two_pipeline_cases() -> Vec<PipelineCase> {
    let caps = FfmpegCaps::default();
    vec![
        PipelineCase {
            name: "01_remux_copy_video_copy_audio",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Heat (1995)/Heat (1995).mkv"),
                start_ms: 0,
                video: VideoOp::Copy,
                audio: AudioOp::Copy { index: 1 },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "02_remux_copy_video_transcode_aac_stereo",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/TrueHD (2022).mkv"),
                start_ms: 0,
                video: VideoOp::Copy,
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 256_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "03_remux_copy_video_transcode_ac3_5_1",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Atmos (2023).mkv"),
                start_ms: 0,
                video: VideoOp::Copy,
                audio: AudioOp::Encode {
                    index: 2,
                    codec: AudioCodec::Ac3,
                    channels: 6,
                    bitrate: 640_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "04_remux_copy_video_transcode_opus",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Concert (2021).mkv"),
                start_ms: 0,
                video: VideoOp::Copy,
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Opus,
                    channels: 2,
                    bitrate: 192_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "05_sw_h264_1080p_copy_audio",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/HEVC_Main10 (2020).mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: None,
                    max_bitrate: 8_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Copy { index: 1 },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "06_sw_h264_scale_720p_aac",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/4K_Remux (2024).mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1280),
                    max_bitrate: 4_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 192_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "07_sw_h264_scale_480p_low_bitrate",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/TV/Show/Season 01/Show - S01E01.mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(854),
                    max_bitrate: 1_500_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 128_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "08_sw_h264_deinterlace_yadif",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/TV/Broadcast_1080i.ts"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: None,
                    max_bitrate: 6_000_000,
                    deinterlace: true,
                    tonemap: false,
                },
                audio: AudioOp::Copy { index: 1 },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "09_sw_h264_deinterlace_and_scale_720p",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/TV/Broadcast_1080i.ts"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1280),
                    max_bitrate: 3_500_000,
                    deinterlace: true,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 192_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "10_sw_h264_burn_text_subtitle_simple",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Anime/Episode_01.mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: None,
                    max_bitrate: 8_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Copy { index: 1 },
                subtitle: SubtitleOp::BurnText { index: 0 },
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "11_sw_h264_burn_text_subtitle_special_chars_path",
            pipeline: Pipeline {
                input: Utf8PathBuf::from(
                    "/media/Movies/Mission: Impossible [2023]/Part 1, 'Director's Cut';Final.mkv",
                ),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1920),
                    max_bitrate: 8_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Copy { index: 1 },
                subtitle: SubtitleOp::BurnText { index: 2 },
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "12_sw_h264_burn_text_subtitle_windows_drive_path",
            pipeline: Pipeline {
                input: Utf8PathBuf::from(r"D:\Media\Movies\Blade Runner 2049 [4K]\Movie: IMAX.mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1920),
                    max_bitrate: 10_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 256_000,
                },
                subtitle: SubtitleOp::BurnText { index: 0 },
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "13_sw_h264_burn_image_subtitle_pgs",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Bluray_Rip (2019).mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: None,
                    max_bitrate: 10_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Copy { index: 1 },
                subtitle: SubtitleOp::BurnImage { index: 3 },
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "14_sw_h264_burn_image_subtitle_deinterlace_and_scale",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/DVD_Interlaced.mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1280),
                    max_bitrate: 4_000_000,
                    deinterlace: true,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 192_000,
                },
                subtitle: SubtitleOp::BurnImage { index: 2 },
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "15_seek_restart_segment_20_copyts",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Heat (1995)/Heat (1995).mkv"),
                start_ms: 120_000,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1920),
                    max_bitrate: 8_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 256_000,
                },
                subtitle: SubtitleOp::None,
                output: HlsOutput {
                    dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-seek"),
                    segment_secs: 6,
                    start_number: 20,
                    renditions: vec![Rendition {
                        name: "0".to_owned(),
                        width: Some(1920),
                        max_bitrate: 8_000_000,
                    }],
                },
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "16_seek_fractional_ms_remux",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/FastStart.mkv"),
                start_ms: 45_250,
                video: VideoOp::Copy,
                audio: AudioOp::Copy { index: 1 },
                subtitle: SubtitleOp::None,
                output: HlsOutput {
                    dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-remux-seek"),
                    segment_secs: 3,
                    start_number: 15,
                    renditions: vec![],
                },
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "17_pi_3s_segments_superfast_threads_4",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Indie (2022).mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1280),
                    max_bitrate: 3_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 160_000,
                },
                subtitle: SubtitleOp::None,
                output: HlsOutput {
                    dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-pi"),
                    segment_secs: 3,
                    start_number: 0,
                    renditions: vec![Rendition {
                        name: "720p".to_owned(),
                        width: Some(1280),
                        max_bitrate: 3_000_000,
                    }],
                },
                backend: Backend::Software,
            },
            caps: FfmpegCaps { preset: "superfast".to_owned(), crf: 23, threads: Some(4) },
        },
        PipelineCase {
            name: "18_sw_hevc_hvc1_encode",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Source.mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::Hevc,
                    width: Some(1920),
                    max_bitrate: 5_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 256_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "19_sw_av1_encode",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Movies/Source.mkv"),
                start_ms: 0,
                video: VideoOp::Encode {
                    codec: VideoCodec::Av1,
                    width: Some(1280),
                    max_bitrate: 2_500_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Opus,
                    channels: 2,
                    bitrate: 160_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "20_audio_only_hls_opus",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/Music/Artist/Album/01 - Track.flac"),
                start_ms: 0,
                video: VideoOp::None,
                audio: AudioOp::Encode {
                    index: 0,
                    codec: AudioCodec::Opus,
                    channels: 2,
                    bitrate: 192_000,
                },
                subtitle: SubtitleOp::None,
                output: default_output(),
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "21_remote_http_worker_source_url",
            pipeline: Pipeline {
                input: Utf8PathBuf::from(
                    "http://127.0.0.1:8420/api/v1/stream/direct/42?token=signed",
                ),
                start_ms: 18_000,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1280),
                    max_bitrate: 4_000_000,
                    deinterlace: false,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 192_000,
                },
                subtitle: SubtitleOp::None,
                output: HlsOutput {
                    dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-worker"),
                    segment_secs: 6,
                    start_number: 3,
                    renditions: vec![],
                },
                backend: Backend::Software,
            },
            caps: caps.clone(),
        },
        PipelineCase {
            name: "22_deinterlace_burn_text_and_scale_720p",
            pipeline: Pipeline {
                input: Utf8PathBuf::from("/media/TV/Classic [1998]/Episode: 01.mkv"),
                start_ms: 6_000,
                video: VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(1280),
                    max_bitrate: 3_500_000,
                    deinterlace: true,
                    tonemap: false,
                },
                audio: AudioOp::Encode {
                    index: 1,
                    codec: AudioCodec::Aac,
                    channels: 2,
                    bitrate: 192_000,
                },
                subtitle: SubtitleOp::BurnText { index: 0 },
                output: HlsOutput {
                    dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-classic"),
                    segment_secs: 6,
                    start_number: 1,
                    renditions: vec![Rendition {
                        name: "0".to_owned(),
                        width: Some(1280),
                        max_bitrate: 3_500_000,
                    }],
                },
                backend: Backend::Software,
            },
            caps,
        },
    ]
}

#[test]
fn snapshot_twenty_two_pipelines() {
    let cases = twenty_two_pipeline_cases();
    assert!(cases.len() >= 20);
    let rendered: Vec<(&str, Vec<String>)> =
        cases.iter().map(|c| (c.name, args_to_strings(&c.pipeline, &c.caps))).collect();
    insta::assert_debug_snapshot!("pipeline_args_matrix", rendered);
}

#[test]
fn rejects_invalid_pipeline_combinations() {
    let caps = FfmpegCaps::default();

    // Subtitle burn-in with VideoOp::Copy is invalid.
    let bad_burn = Pipeline {
        input: Utf8PathBuf::from("/media/Movie.mkv"),
        start_ms: 0,
        video: VideoOp::Copy,
        audio: AudioOp::Copy { index: 1 },
        subtitle: SubtitleOp::BurnText { index: 0 },
        output: default_output(),
        backend: Backend::Software,
    };
    assert!(build_args(&bad_burn, &caps).is_err());

    // Tone mapping on Pi profile (PiDrmDecode or V4l2m2m) is rejected (M4.10).
    let bad_pi_tonemap = Pipeline {
        input: Utf8PathBuf::from("/media/Movie_HDR.mkv"),
        start_ms: 0,
        video: VideoOp::Encode {
            codec: VideoCodec::H264,
            width: None,
            max_bitrate: 8_000_000,
            deinterlace: false,
            tonemap: true,
        },
        audio: AudioOp::Copy { index: 1 },
        subtitle: SubtitleOp::None,
        output: default_output(),
        backend: Backend::PiDrmDecode,
    };
    assert!(build_args(&bad_pi_tonemap, &caps).is_err());

    // Multi-rendition ABR ladder on Pi or Software backend is rejected (M4.13).
    let bad_abr_pi = Pipeline {
        input: Utf8PathBuf::from("/media/Movie.mkv"),
        start_ms: 0,
        video: VideoOp::Encode {
            codec: VideoCodec::H264,
            width: Some(1920),
            max_bitrate: 8_000_000,
            deinterlace: false,
            tonemap: false,
        },
        audio: AudioOp::Copy { index: 1 },
        subtitle: SubtitleOp::None,
        output: HlsOutput {
            dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-abr"),
            segment_secs: 6,
            start_number: 0,
            renditions: vec![
                Rendition { name: "0".to_owned(), width: Some(1920), max_bitrate: 8_000_000 },
                Rendition { name: "1".to_owned(), width: Some(1280), max_bitrate: 4_000_000 },
            ],
        },
        backend: Backend::PiDrmDecode,
    };
    assert!(build_args(&bad_abr_pi, &caps).is_err());

    // Zero segment_secs rejected.
    let mut bad_seg = bad_burn.clone();
    bad_seg.subtitle = SubtitleOp::None;
    bad_seg.output.segment_secs = 0;
    assert!(build_args(&bad_seg, &caps).is_err());
}

#[test]
fn hardware_backends_tonemap_and_abr_ladder_args() {
    let caps = FfmpegCaps::default();

    // M4.4 + M4.10: VAAPI with HDR tone-mapping and HEVC encode
    let vaapi_hdr = Pipeline {
        input: Utf8PathBuf::from("/media/Movies/HDR10.mkv"),
        start_ms: 0,
        video: VideoOp::Encode {
            codec: VideoCodec::Hevc,
            width: Some(1920),
            max_bitrate: 8_000_000,
            deinterlace: false,
            tonemap: true,
        },
        audio: AudioOp::Copy { index: 1 },
        subtitle: SubtitleOp::None,
        output: default_output(),
        backend: Backend::Vaapi,
    };
    let vaapi_args = args_to_strings(&vaapi_hdr, &caps);
    assert!(vaapi_args.windows(2).any(|w| w == ["-hwaccel", "vaapi"]));
    assert!(vaapi_args.windows(2).any(|w| w == ["-c:v", "hevc_vaapi"]));
    assert!(vaapi_args.iter().any(|a| a.contains("tonemap_vaapi") && a.contains("scale_vaapi")));

    // M4.5 + M4.10: QSV with deinterlace + tone-mapping + low_power=1
    let qsv = Pipeline {
        backend: Backend::Qsv,
        video: VideoOp::Encode {
            codec: VideoCodec::H264,
            width: Some(1280),
            max_bitrate: 4_000_000,
            deinterlace: true,
            tonemap: true,
        },
        ..vaapi_hdr.clone()
    };
    let qsv_args = args_to_strings(&qsv, &caps);
    assert!(qsv_args.windows(2).any(|w| w == ["-hwaccel", "qsv"]));
    assert!(qsv_args.windows(2).any(|w| w == ["-c:v", "h264_qsv"]));
    assert!(qsv_args.windows(2).any(|w| w == ["-low_power", "1"]));
    assert!(
        qsv_args
            .iter()
            .any(|a| a.contains("vpp_qsv=deinterlace=2:tonemap=1:w=1280:h=-2:format=nv12"))
    );

    // M4.6 + M4.10: NVENC with yadif_cuda + tonemap_cuda + scale_cuda + av1_nvenc
    let nvenc = Pipeline {
        backend: Backend::Nvenc,
        video: VideoOp::Encode {
            codec: VideoCodec::Av1,
            width: Some(1920),
            max_bitrate: 6_000_000,
            deinterlace: true,
            tonemap: true,
        },
        ..vaapi_hdr.clone()
    };
    let nvenc_args = args_to_strings(&nvenc, &caps);
    assert!(nvenc_args.windows(2).any(|w| w == ["-hwaccel", "cuda"]));
    assert!(nvenc_args.windows(2).any(|w| w == ["-c:v", "av1_nvenc"]));
    assert!(nvenc_args.iter().any(|a| a.contains("yadif_cuda")
        && a.contains("tonemap_cuda")
        && a.contains("scale_cuda")));

    // M4.7: Pi 4 V4L2M2M and Pi 5 DRM decode + libx264 software encode
    let pi4 = Pipeline {
        backend: Backend::V4l2m2m,
        video: VideoOp::Encode {
            codec: VideoCodec::H264,
            width: Some(1280),
            max_bitrate: 4_000_000,
            deinterlace: false,
            tonemap: false,
        },
        ..vaapi_hdr.clone()
    };
    let pi4_args = args_to_strings(&pi4, &caps);
    assert!(pi4_args.windows(2).any(|w| w == ["-c:v", "h264_v4l2m2m"]));

    let pi5 = Pipeline { backend: Backend::PiDrmDecode, ..pi4.clone() };
    let pi5_args = args_to_strings(&pi5, &caps);
    assert!(pi5_args.windows(2).any(|w| w == ["-hwaccel", "drm"]));
    assert!(pi5_args.windows(2).any(|w| w == ["-c:v", "libx264"]));

    // M4.8: Windows D3D11VA + AMF
    let amf = Pipeline { backend: Backend::D3d11Amf, ..pi4.clone() };
    let amf_args = args_to_strings(&amf, &caps);
    assert!(amf_args.windows(2).any(|w| w == ["-hwaccel", "d3d11va"]));
    assert!(amf_args.windows(2).any(|w| w == ["-c:v", "h264_amf"]));

    // M4.13: Multi-rendition GPU ABR ladder with split=3
    let abr = Pipeline {
        input: Utf8PathBuf::from("/media/Movies/4K_Source.mkv"),
        start_ms: 0,
        video: VideoOp::Encode {
            codec: VideoCodec::H264,
            width: Some(1920),
            max_bitrate: 8_000_000,
            deinterlace: false,
            tonemap: false,
        },
        audio: AudioOp::Encode { index: 1, codec: AudioCodec::Aac, channels: 2, bitrate: 192_000 },
        subtitle: SubtitleOp::None,
        output: HlsOutput {
            dir: Utf8PathBuf::from("/var/lib/mbl/transcode/sess-abr"),
            segment_secs: 6,
            start_number: 0,
            renditions: vec![
                Rendition { name: "0".to_owned(), width: Some(1920), max_bitrate: 8_000_000 },
                Rendition { name: "1".to_owned(), width: Some(1280), max_bitrate: 4_000_000 },
                Rendition { name: "2".to_owned(), width: Some(854), max_bitrate: 1_500_000 },
            ],
        },
        backend: Backend::Vaapi,
    };
    let abr_args = args_to_strings(&abr, &caps);
    assert!(abr_args.iter().any(|a| a.contains("[0:v:0]split=3[s0][s1][s2]")));
    assert!(abr_args.iter().any(|a| a.ends_with("/0/ffmpeg.m3u8")));
    assert!(abr_args.iter().any(|a| a.ends_with("/1/ffmpeg.m3u8")));
    assert!(abr_args.iter().any(|a| a.ends_with("/2/ffmpeg.m3u8")));
}

#[tokio::test]
async fn transcodes_10s_test_media_to_fmp4_hls_and_verifies_with_ffprobe() {
    let Ok(tools) = mbl_probe::FfTools::locate("", "").await else {
        eprintln!("skipping: ffmpeg/ffprobe not found on PATH");
        return;
    };

    let tmp = tempfile::tempdir().unwrap();
    let input_path = tmp.path().join("source_10s.mkv");

    // Generate a 10-second synthetic MKV (MPEG-4 Part 2 video + FLAC audio) so both video and
    // audio are genuinely transcoded to H.264 + AAC in fMP4 HLS segments.
    let gen_status = tokio::process::Command::new(&tools.ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=10:size=640x360:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=10",
            "-c:v",
            "mpeg4",
            "-q:v",
            "5",
            "-c:a",
            "flac",
            input_path.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .await
        .unwrap();
    assert!(gen_status.success(), "failed to generate 10s synthetic input");

    let out_dir = tmp.path().join("hls_out");
    let rendition_dir = out_dir.join("0");
    std::fs::create_dir_all(&rendition_dir).unwrap();

    let pipeline = Pipeline {
        input: Utf8PathBuf::from_path_buf(input_path).unwrap(),
        start_ms: 0,
        video: VideoOp::Encode {
            codec: VideoCodec::H264,
            width: Some(320),
            max_bitrate: 800_000,
            deinterlace: false,
            tonemap: false,
        },
        audio: AudioOp::Encode { index: 1, codec: AudioCodec::Aac, channels: 2, bitrate: 128_000 },
        subtitle: SubtitleOp::None,
        output: HlsOutput {
            dir: Utf8PathBuf::from_path_buf(out_dir).unwrap(),
            segment_secs: 2,
            start_number: 0,
            renditions: vec![Rendition {
                name: "0".to_owned(),
                width: Some(320),
                max_bitrate: 800_000,
            }],
        },
        backend: Backend::Software,
    };

    let caps = FfmpegCaps { preset: "ultrafast".to_owned(), crf: 28, threads: Some(2) };
    let args = build_args(&pipeline, &caps).unwrap();

    let output = tokio::process::Command::new(&tools.ffmpeg)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "ffmpeg transcode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify fMP4 init segment and media segments exist.
    assert!(rendition_dir.join("init.mp4").exists(), "init.mp4 must be written");
    assert!(rendition_dir.join("0.m4s").exists(), "0.m4s must be written");
    assert!(rendition_dir.join("4.m4s").exists(), "4.m4s must be written (5 x 2s = 10s)");

    // Probe the generated HLS playlist with ffprobe and verify codecs, dimensions, and duration.
    let playlist_path = rendition_dir.join("ffmpeg.m3u8");
    let probed = tools.probe(&playlist_path, std::time::Duration::from_secs(15)).await.unwrap();
    assert!(
        (9_500..=10_500).contains(&probed.duration_ms),
        "expected ~10000ms duration, got {}",
        probed.duration_ms
    );

    let video = probed.streams.iter().find(|s| s.kind == StreamKind::Video).unwrap();
    assert_eq!(video.codec, "h264");
    assert_eq!(video.width, Some(320));
    assert_eq!(video.height, Some(180));

    let audio = probed.streams.iter().find(|s| s.kind == StreamKind::Audio).unwrap();
    assert_eq!(audio.codec, "aac");
    assert_eq!(audio.channels, Some(2));
}
