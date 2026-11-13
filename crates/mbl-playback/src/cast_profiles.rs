//! Server-owned, conservative SDR/H.264 Cast profiles, revision 1.
//! Model limits: https://developers.google.com/cast/docs/media (2026-11-12).
//! Higher codec/HDR capabilities require receiver verification before enabling.

pub const REVISION: u32 = 1;
pub const NAMES: &[&str] = &[
    "cast-safe-720",
    "nest-hub",
    "nest-hub-max",
    "chromecast-legacy",
    "chromecast-gen3",
    "chromecast-ultra",
    "google-tv-hd",
    "google-tv-4k",
    "google-tv-streamer",
];

/// Width, height, maximum frames per second. Entries are combinations, not
/// independent maxima (legacy Chromecast supports 1080p30 OR 720p60).
pub fn modes(name: &str) -> &'static [(u32, u32, u32)] {
    match name {
        "cast-safe-720" | "nest-hub-max" | "google-tv-hd" => &[(1280, 720, 30)],
        "nest-hub" => &[(1280, 720, 60)],
        "chromecast-legacy" => &[(1920, 1080, 30), (1280, 720, 60)],
        "chromecast-gen3" | "chromecast-ultra" => &[(1920, 1080, 60)],
        "google-tv-4k" => &[(3840, 2160, 30), (1920, 1080, 60)],
        "google-tv-streamer" => &[(3840, 2160, 60)],
        _ => &[],
    }
}

pub fn level(name: &str) -> i32 {
    match name {
        "google-tv-streamer" => 52,
        "google-tv-4k" => 51,
        "chromecast-gen3" | "chromecast-ultra" | "google-tv-hd" => 42,
        _ => 41,
    }
}
