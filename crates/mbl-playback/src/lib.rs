//! Device profiles and the playback decision engine.

pub mod profile;

pub use profile::{
    BUILTIN_PROFILES, CodecLimit, DeviceProfile, DeviceProfileOverlay, DirectPlayRule,
    SubtitleMethod, SubtitleRule, TranscodeTarget,
};
