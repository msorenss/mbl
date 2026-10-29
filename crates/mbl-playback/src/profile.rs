//! Device capability profiles (`profiles/*.toml`) and dynamic profile merging.

use mbl_core::{Error, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const CHROME_TOML: &str = include_str!("../profiles/chrome.toml");
const FIREFOX_TOML: &str = include_str!("../profiles/firefox.toml");
const SAFARI_TOML: &str = include_str!("../profiles/safari.toml");
const EDGE_TOML: &str = include_str!("../profiles/edge.toml");
const GENERIC_HLS_TOML: &str = include_str!("../profiles/generic-hls.toml");

/// Names of all built-in device profiles.
pub const BUILTIN_PROFILES: &[&str] =
    &["chrome", "firefox", "safari", "edge", "generic-hls", "chromecast"];

/// Full capability description of a playback client (Appendix A.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceProfile {
    pub name: String,
    #[serde(alias = "direct_play")]
    pub direct_play: Vec<DirectPlayRule>,
    #[serde(default, alias = "codec_limits")]
    pub codec_limits: Vec<CodecLimit>,
    #[serde(alias = "transcode_target")]
    pub transcode_target: TranscodeTarget,
    #[serde(default)]
    pub subtitles: Vec<SubtitleRule>,
    #[serde(alias = "max_audio_channels")]
    pub max_audio_channels: u8,
    #[serde(default, alias = "max_bitrate")]
    pub max_bitrate: Option<u64>,
}

/// Container and codec combinations that the client can play directly over HTTP range requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectPlayRule {
    pub containers: Vec<String>,
    #[serde(default, alias = "video_codecs")]
    pub video_codecs: Vec<String>,
    #[serde(default, alias = "audio_codecs")]
    pub audio_codecs: Vec<String>,
}

/// Per-video-codec capability limits enforced before direct play or direct stream (remux).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodecLimit {
    pub codec: String,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default, alias = "max_level")]
    pub max_level: Option<i32>,
    #[serde(default, alias = "max_width")]
    pub max_width: Option<u32>,
    #[serde(default, alias = "max_height")]
    pub max_height: Option<u32>,
    #[serde(default, alias = "max_bit_depth")]
    pub max_bit_depth: Option<u8>,
    #[serde(default)]
    pub hdr: bool,
    #[serde(default, alias = "hdr_formats")]
    pub hdr_formats: Vec<String>,
}

/// Target HLS container and ordered codec preferences used for remux and transcoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscodeTarget {
    pub container: String,
    #[serde(alias = "video_codecs")]
    pub video_codecs: Vec<String>,
    #[serde(alias = "audio_codecs")]
    pub audio_codecs: Vec<String>,
}

/// How a subtitle format can be delivered to the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleMethod {
    #[serde(alias = "sidecar")]
    External,
    #[serde(alias = "embedded")]
    Embed,
    #[serde(alias = "burnin", alias = "burn_in")]
    Burn,
}

/// Delivery rule for one subtitle codec/format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubtitleRule {
    pub format: String,
    pub method: SubtitleMethod,
    #[serde(default, alias = "target_format")]
    pub target_format: Option<String>,
}

/// Dynamic profile sent by a client in a playback request, merged over a built-in profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceProfileOverlay {
    /// Built-in profile name to use as the base (or custom name when `base` is set).
    pub name: String,
    /// Optional explicit built-in base profile name if `name` is custom.
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default, alias = "direct_play")]
    pub direct_play: Option<Vec<DirectPlayRule>>,
    #[serde(default, alias = "codec_limits")]
    pub codec_limits: Option<Vec<CodecLimit>>,
    #[serde(default, alias = "transcode_target")]
    pub transcode_target: Option<TranscodeTarget>,
    #[serde(default)]
    pub subtitles: Option<Vec<SubtitleRule>>,
    #[serde(default, alias = "max_audio_channels")]
    pub max_audio_channels: Option<u8>,
    #[serde(default, alias = "max_bitrate")]
    pub max_bitrate: Option<u64>,
}

impl DeviceProfile {
    /// Parse and validate a `DeviceProfile` from TOML text.
    pub fn from_toml(src: &str) -> Result<Self> {
        let profile: Self = toml::from_str(src)
            .map_err(|e| Error::Validation(format!("invalid device profile TOML: {e}")))?;
        profile.validate()?;
        Ok(profile)
    }

    /// Look up one of the built-in profiles (`chrome`, `firefox`, `safari`, `edge`, `generic-hls`).
    pub fn builtin(name: &str) -> Result<Self> {
        if name.eq_ignore_ascii_case("chromecast") {
            let mut profile = Self::from_toml(GENERIC_HLS_TOML)?;
            profile.name = "chromecast".into();
            return Ok(profile);
        }
        let raw = match name.trim().to_ascii_lowercase().as_str() {
            "chrome" => CHROME_TOML,
            "firefox" => FIREFOX_TOML,
            "safari" => SAFARI_TOML,
            "edge" => EDGE_TOML,
            "generic-hls" | "generic" => GENERIC_HLS_TOML,
            other => {
                return Err(Error::Validation(format!(
                    "unknown built-in device profile '{other}' (expected one of: {})",
                    BUILTIN_PROFILES.join(", ")
                )));
            }
        };
        Self::from_toml(raw)
    }

    /// Validate all structural invariants of a profile.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::Validation("device profile name must not be empty".into()));
        }
        if self.max_audio_channels == 0 {
            return Err(Error::Validation("max_audio_channels must be at least 1".into()));
        }
        if self.max_bitrate == Some(0) {
            return Err(Error::Validation("max_bitrate must be greater than 0".into()));
        }
        if self.direct_play.is_empty() {
            return Err(Error::Validation(
                "device profile must define at least one direct_play rule".into(),
            ));
        }
        for rule in &self.direct_play {
            if rule.containers.is_empty() || rule.containers.iter().any(|c| c.trim().is_empty()) {
                return Err(Error::Validation(
                    "direct_play rule must have non-empty containers".into(),
                ));
            }
            if rule.video_codecs.is_empty() && rule.audio_codecs.is_empty() {
                return Err(Error::Validation(
                    "direct_play rule must list at least one video or audio codec".into(),
                ));
            }
            if rule.video_codecs.iter().chain(&rule.audio_codecs).any(|c| c.trim().is_empty()) {
                return Err(Error::Validation("direct_play codec names must not be empty".into()));
            }
        }
        for limit in &self.codec_limits {
            if limit.codec.trim().is_empty() {
                return Err(Error::Validation("codec_limits codec must not be empty".into()));
            }
            if limit.max_level.is_some_and(|l| l <= 0) {
                return Err(Error::Validation(format!(
                    "codec_limits for '{}' has invalid max_level",
                    limit.codec
                )));
            }
            if limit.max_width == Some(0) || limit.max_height == Some(0) {
                return Err(Error::Validation(format!(
                    "codec_limits for '{}' has zero max dimension",
                    limit.codec
                )));
            }
            if limit.max_bit_depth == Some(0) {
                return Err(Error::Validation(format!(
                    "codec_limits for '{}' has zero max_bit_depth",
                    limit.codec
                )));
            }
            if !limit.hdr && !limit.hdr_formats.is_empty() {
                return Err(Error::Validation(format!(
                    "codec_limits for '{}' lists hdr_formats while hdr is false",
                    limit.codec
                )));
            }
        }
        if self.transcode_target.container.trim().is_empty() {
            return Err(Error::Validation("transcode_target.container must not be empty".into()));
        }
        if self.transcode_target.video_codecs.is_empty()
            || self.transcode_target.video_codecs.iter().any(|c| c.trim().is_empty())
        {
            return Err(Error::Validation(
                "transcode_target.video_codecs must be non-empty".into(),
            ));
        }
        if self.transcode_target.audio_codecs.is_empty()
            || self.transcode_target.audio_codecs.iter().any(|c| c.trim().is_empty())
        {
            return Err(Error::Validation(
                "transcode_target.audio_codecs must be non-empty".into(),
            ));
        }
        for sub in &self.subtitles {
            if sub.format.trim().is_empty() {
                return Err(Error::Validation("subtitle rule format must not be empty".into()));
            }
            if let Some(target) = &sub.target_format
                && target.trim().is_empty()
            {
                return Err(Error::Validation(
                    "subtitle rule target_format must not be empty".into(),
                ));
            }
        }
        Ok(())
    }

    /// Merge a dynamic client overlay over `self`, validating the resulting profile.
    pub fn merge(&self, overlay: DeviceProfileOverlay) -> Result<Self> {
        let mut merged = self.clone();
        if !overlay.name.trim().is_empty() {
            merged.name = overlay.name;
        }
        if let Some(direct_play) = overlay.direct_play {
            merged.direct_play = direct_play;
        }
        if let Some(limits) = overlay.codec_limits {
            for limit in limits {
                if let Some(existing) = merged
                    .codec_limits
                    .iter_mut()
                    .find(|l| l.codec.eq_ignore_ascii_case(&limit.codec))
                {
                    *existing = limit;
                } else {
                    merged.codec_limits.push(limit);
                }
            }
        }
        if let Some(target) = overlay.transcode_target {
            merged.transcode_target = target;
        }
        if let Some(subs) = overlay.subtitles {
            for sub in subs {
                if let Some(existing) =
                    merged.subtitles.iter_mut().find(|s| s.format.eq_ignore_ascii_case(&sub.format))
                {
                    *existing = sub;
                } else {
                    merged.subtitles.push(sub);
                }
            }
        }
        if let Some(ch) = overlay.max_audio_channels {
            merged.max_audio_channels = ch;
        }
        if let Some(br) = overlay.max_bitrate {
            merged.max_bitrate = Some(br);
        }
        merged.validate()?;
        Ok(merged)
    }

    /// Find the `CodecLimit` for a video codec (case-insensitive).
    pub fn codec_limit(&self, codec: &str) -> Option<&CodecLimit> {
        self.codec_limits.iter().find(|l| l.codec.eq_ignore_ascii_case(codec))
    }

    /// Find the `SubtitleRule` for a subtitle codec/format (case-insensitive).
    pub fn subtitle_rule(&self, format: &str) -> Option<&SubtitleRule> {
        self.subtitles.iter().find(|s| s.format.eq_ignore_ascii_case(format))
    }
}

impl DeviceProfileOverlay {
    /// Resolve this dynamic profile by merging it over the matching built-in profile.
    pub fn resolve(self) -> Result<DeviceProfile> {
        let base_name = self.base.as_deref().unwrap_or(&self.name);
        if base_name.trim().is_empty() {
            return Err(Error::Validation("device profile name must not be empty".into()));
        }
        let base = DeviceProfile::builtin(base_name)?;
        base.merge(self)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn all_builtin_profiles_parse_and_validate() {
        for &name in BUILTIN_PROFILES {
            let p = DeviceProfile::builtin(name).unwrap();
            assert_eq!(p.name, name);
            assert!(!p.direct_play.is_empty());
            assert!(!p.codec_limits.is_empty());
            assert_eq!(p.transcode_target.container, "fmp4");
            assert!(!p.subtitles.is_empty());
        }
    }

    #[test]
    fn builtin_capabilities_match_browser_expectations() {
        let chrome = DeviceProfile::builtin("chrome").unwrap();
        assert!(chrome.codec_limit("hevc").is_some());
        assert!(chrome.codec_limit("av1").is_some());

        let firefox = DeviceProfile::builtin("firefox").unwrap();
        assert!(firefox.codec_limit("hevc").is_none());
        assert!(firefox.codec_limit("av1").is_some());

        let safari = DeviceProfile::builtin("safari").unwrap();
        let hevc = safari.codec_limit("hevc").unwrap();
        assert!(hevc.hdr);
        assert!(hevc.hdr_formats.iter().any(|f| f == "dv"));

        let generic = DeviceProfile::builtin("generic-hls").unwrap();
        assert_eq!(generic.max_audio_channels, 2);
        assert_eq!(generic.codec_limit("h264").unwrap().max_width, Some(1920));
    }

    #[test]
    fn overlay_merges_over_builtin_profile() {
        let overlay: DeviceProfileOverlay = serde_json::from_str(
            r#"{
                "name": "firefox",
                "maxAudioChannels": 2,
                "maxBitrate": 8000000,
                "codecLimits": [
                    {
                        "codec": "hevc",
                        "profiles": ["Main"],
                        "maxWidth": 1920,
                        "maxHeight": 1080,
                        "maxBitDepth": 8,
                        "hdr": false
                    },
                    {
                        "codec": "h264",
                        "profiles": ["Baseline", "Main"],
                        "maxLevel": 40,
                        "maxWidth": 1920,
                        "maxHeight": 1080,
                        "maxBitDepth": 8,
                        "hdr": false
                    }
                ],
                "subtitles": [
                    { "format": "ass", "method": "external", "targetFormat": "ass" }
                ]
            }"#,
        )
        .unwrap();

        let resolved = overlay.resolve().unwrap();
        assert_eq!(resolved.name, "firefox");
        assert_eq!(resolved.max_audio_channels, 2);
        assert_eq!(resolved.max_bitrate, Some(8_000_000));
        assert_eq!(resolved.codec_limit("h264").unwrap().max_level, Some(40));
        assert_eq!(resolved.codec_limit("hevc").unwrap().max_width, Some(1920));
        assert!(resolved.codec_limit("av1").is_some(), "unmodified limits are preserved");
        assert_eq!(resolved.subtitle_rule("ass").unwrap().target_format.as_deref(), Some("ass"));
        assert_eq!(
            resolved.subtitle_rule("subrip").unwrap().target_format.as_deref(),
            Some("vtt"),
            "other subtitle rules are preserved"
        );
    }

    #[test]
    fn invalid_profiles_are_rejected_with_clear_errors() {
        // Unknown built-in name
        let err = DeviceProfile::builtin("netscape").unwrap_err();
        assert!(err.to_string().contains("unknown built-in device profile"));

        // Unknown TOML field
        let err = DeviceProfile::from_toml(
            r#"
            name = "bad"
            max_audio_channels = 2
            bogus_field = true
            [transcode_target]
            container = "fmp4"
            video_codecs = ["h264"]
            audio_codecs = ["aac"]
            "#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("bogus_field"));

        // Zero audio channels
        let err = DeviceProfileOverlay {
            name: "chrome".into(),
            max_audio_channels: Some(0),
            ..Default::default()
        }
        .resolve()
        .unwrap_err();
        assert!(err.to_string().contains("max_audio_channels"));

        // Zero bitrate
        let err = DeviceProfileOverlay {
            name: "chrome".into(),
            max_bitrate: Some(0),
            ..Default::default()
        }
        .resolve()
        .unwrap_err();
        assert!(err.to_string().contains("max_bitrate"));

        // Empty direct_play
        let err = DeviceProfileOverlay {
            name: "chrome".into(),
            direct_play: Some(vec![]),
            ..Default::default()
        }
        .resolve()
        .unwrap_err();
        assert!(err.to_string().contains("direct_play"));

        // hdr_formats with hdr = false
        let err = DeviceProfileOverlay {
            name: "chrome".into(),
            codec_limits: Some(vec![CodecLimit {
                codec: "h264".into(),
                profiles: vec![],
                max_level: None,
                max_width: None,
                max_height: None,
                max_bit_depth: None,
                hdr: false,
                hdr_formats: vec!["hdr10".into()],
            }]),
            ..Default::default()
        }
        .resolve()
        .unwrap_err();
        assert!(err.to_string().contains("hdr_formats"));
    }
}
