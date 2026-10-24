//! Device profiles and the playback decision engine.

pub mod decide;
pub mod profile;

pub use decide::{
    Decision, DecisionContext, HwSummary, Network, PlayMethod, Reason, StreamAction,
    SubtitleDelivery, decide,
};
pub use profile::{
    BUILTIN_PROFILES, CodecLimit, DeviceProfile, DeviceProfileOverlay, DirectPlayRule,
    SubtitleMethod, SubtitleRule, TranscodeTarget,
};
