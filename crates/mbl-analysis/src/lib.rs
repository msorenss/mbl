//! Media intelligence & analysis (M5.4–M5.7):
//! - Optimized versions (`optimize`)
//! - Keyframe trickplay sprite sheets (`trickplay`)
//! - Audio fingerprint + chapter intro detection (`intro`)
//! - Chapter + `blackdetect`/`silencedetect` credits detection (`credits`)

pub mod audio_stream;
pub mod credits;
pub mod features;
pub mod intro;
pub mod loudness;
pub mod optimize;
pub mod subsync;
pub mod trickplay;

pub use audio_stream::{AudioCodec, build_audio_transcode_args};
pub use credits::{
    detect_credits, detect_credits_from_chapters, detect_credits_from_filter_log,
    run_credits_detection,
};
pub use features::{
    ClipFormat, EpisodeContext, SceneActor, SceneSong, SceneXRay, SleepGuardConfig,
    SleepGuardDecision, SpoilerFreeRecapPrompt, build_clip_ffmpeg_args, build_spoiler_free_recap,
    check_sleep_guard,
};
pub use intro::{
    DetectedMarker, EpisodeFingerprint, detect_intro_from_chapters, find_common_intros,
    fingerprint_media_file, fingerprint_pcm_s16le,
};
pub use loudness::{
    LoudnessResult, analyze_loudness, build_ebur128_ffmpeg_args, parse_ebur128_output,
};
pub use optimize::{
    OptimizationEstimate, OptimizationProfile, build_optimize_ffmpeg_args, estimate_optimization,
    is_within_night_window, transcode_optimized_mp4,
};
pub use subsync::{
    SpeechSegment, SubtitleCue, align_srt_content, find_alignment_offset, format_timestamp,
    parse_srt, parse_timestamp, serialize_srt,
};
pub use trickplay::{
    TrickplayManifest, TrickplaySprite, TrickplayTile, build_manifest, build_trickplay_ffmpeg_args,
    generate_trickplay,
};
