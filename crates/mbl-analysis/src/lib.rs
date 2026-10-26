//! Media intelligence & analysis (M5.4–M5.7):
//! - Optimized versions (`optimize`)
//! - Keyframe trickplay sprite sheets (`trickplay`)
//! - Audio fingerprint + chapter intro detection (`intro`)
//! - Chapter + `blackdetect`/`silencedetect` credits detection (`credits`)

pub mod credits;
pub mod intro;
pub mod optimize;
pub mod trickplay;

pub use credits::{
    detect_credits, detect_credits_from_chapters, detect_credits_from_filter_log,
    run_credits_detection,
};
pub use intro::{
    DetectedMarker, EpisodeFingerprint, detect_intro_from_chapters, find_common_intros,
    fingerprint_media_file, fingerprint_pcm_s16le,
};
pub use optimize::{
    OptimizationEstimate, OptimizationProfile, build_optimize_ffmpeg_args, estimate_optimization,
    is_within_night_window, transcode_optimized_mp4,
};
pub use trickplay::{
    TrickplayManifest, TrickplaySprite, TrickplayTile, build_manifest, build_trickplay_ffmpeg_args,
    generate_trickplay,
};
