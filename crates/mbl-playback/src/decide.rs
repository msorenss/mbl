//! Pure playback decision engine (PLAN.md §8.3, IMPLEMENTATION.md Appendix A.7).

use std::fmt;

use mbl_core::StreamKind;
use mbl_probe::{HdrKind, MediaInfo, StreamInfo};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::profile::{CodecLimit, DeviceProfile, SubtitleMethod};

/// Network classification of the playback session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Network {
    #[default]
    Lan,
    TailnetDirect,
    TailnetRelayed,
    Wan,
}

impl Network {
    pub fn is_remote(self) -> bool {
        !matches!(self, Self::Lan)
    }
}

/// Summary of available hardware acceleration (populated in M4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HwSummary {
    pub backend: String,
    pub encode_codecs: Vec<String>,
    pub can_tonemap: bool,
}

/// Per-request context passed to [`decide`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecisionContext {
    pub max_bitrate: Option<u64>,
    pub network: Network,
    pub can_transcode_video: bool,
    pub hw: Option<HwSummary>,
    pub audio_index: Option<u32>,
    pub subtitle_index: Option<u32>,
}

impl Default for DecisionContext {
    fn default() -> Self {
        Self {
            max_bitrate: None,
            network: Network::Lan,
            can_transcode_video: true,
            hw: None,
            audio_index: None,
            subtitle_index: None,
        }
    }
}

/// Overall delivery method chosen by the decision engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PlayMethod {
    DirectPlay,
    DirectStream,
    Transcode,
}

/// Action to take for an individual video or audio stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum StreamAction {
    Copy,
    Transcode {
        codec: String,
        bitrate: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
    },
    Drop,
}

/// How subtitles will be delivered for the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "delivery", rename_all = "camelCase")]
pub enum SubtitleDelivery {
    None,
    Sidecar { format: String },
    Embedded,
    BurnIn,
}

/// Why a stream could not be direct-played as-is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "code", content = "detail", rename_all = "camelCase")]
pub enum Reason {
    ContainerUnsupported(String),
    VideoCodecUnsupported(String),
    VideoProfileUnsupported(String),
    BitDepthUnsupported(u8),
    HdrUnsupported,
    ResolutionTooHigh,
    BitrateTooHigh { source: u64, limit: u64 },
    AudioCodecUnsupported(String),
    TooManyChannels(u8),
    SubtitleBurnInRequired(String),
    NoTranscodeCapacity,
    Other(String),
}

impl Reason {
    /// Human-readable explanation suitable for the UI and MCP tools.
    pub fn message(&self) -> String {
        match self {
            Self::ContainerUnsupported(c) => {
                format!("Container '{c}' is not supported for direct play on this device")
            }
            Self::VideoCodecUnsupported(c) => {
                format!("Video codec '{c}' is not supported by this device")
            }
            Self::VideoProfileUnsupported(p) => {
                format!("Video profile or level '{p}' is not supported by this device")
            }
            Self::BitDepthUnsupported(b) => {
                format!("{b}-bit video is not supported by this device")
            }
            Self::HdrUnsupported => {
                "HDR format is not supported by this device and requires transcoding".into()
            }
            Self::ResolutionTooHigh => {
                "Video resolution exceeds the maximum supported by this device".into()
            }
            Self::BitrateTooHigh { source, limit } => {
                format!("Source bitrate ({source} bps) exceeds the limit ({limit} bps)")
            }
            Self::AudioCodecUnsupported(c) => {
                format!("Audio codec '{c}' is not supported by this device")
            }
            Self::TooManyChannels(ch) => {
                format!("Audio has {ch} channels, exceeding the device limit")
            }
            Self::SubtitleBurnInRequired(fmt) => {
                format!("Subtitle format '{fmt}' must be burned into the video stream")
            }
            Self::NoTranscodeCapacity => {
                "Video transcoding is required, but the server has no free transcode capacity"
                    .into()
            }
            Self::Other(msg) => msg.clone(),
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

/// Complete playback decision for a media file against a device profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub method: PlayMethod,
    pub container: String,
    pub video: StreamAction,
    pub audio: StreamAction,
    pub subtitle: SubtitleDelivery,
    pub reasons: Vec<Reason>,
}

impl Decision {
    /// True when the decision requires video transcoding that the server cannot currently perform.
    pub fn is_refused(&self) -> bool {
        self.reasons.contains(&Reason::NoTranscodeCapacity)
    }

    /// Human-readable messages for all reasons on this decision.
    pub fn reason_messages(&self) -> Vec<String> {
        self.reasons.iter().map(Reason::message).collect()
    }
}

/// Pure playback decision function (no I/O).
pub fn decide(media: &MediaInfo, profile: &DeviceProfile, ctx: &DecisionContext) -> Decision {
    let video = media.primary_video();
    let audio = select_audio(media, ctx.audio_index);
    let subtitle_stream = ctx
        .subtitle_index
        .and_then(|idx| media.streams_of(StreamKind::Subtitle).find(|s| s.index == idx));

    let effective_max_bitrate = match (ctx.max_bitrate, profile.max_bitrate) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let source_bitrate = media.bitrate.or_else(|| {
        (media.duration_ms > 0 && media.size > 0)
            .then(|| (media.size.saturating_mul(8).saturating_mul(1000)) / media.duration_ms)
    });

    let bitrate_too_high = match (source_bitrate, effective_max_bitrate) {
        (Some(src), Some(limit)) if src > limit => Some((src, limit)),
        _ => None,
    };

    // 1. Subtitle evaluation
    let subtitle = match subtitle_stream {
        None => SubtitleDelivery::None,
        Some(sub) => evaluate_subtitle(sub, profile),
    };

    // 2. Video evaluation
    let mut video_reasons = Vec::new();
    if let Some(v) = video {
        evaluate_video(v, profile, &mut video_reasons);
    }

    // 3. Audio evaluation
    let mut audio_channel_reason = None;
    if let Some(a) = audio {
        let ch = a.channels.unwrap_or(2);
        if ch > profile.max_audio_channels {
            audio_channel_reason = Some(Reason::TooManyChannels(ch));
        }
    }

    // 4. Check if DirectPlay is possible
    let can_direct_play_rule = profile.direct_play.iter().any(|rule| {
        if !container_matches(&media.container, &rule.containers) {
            return false;
        }
        let video_ok = match video {
            Some(v) => rule.video_codecs.iter().any(|c| c.eq_ignore_ascii_case(&v.codec)),
            None => rule.video_codecs.is_empty() || !rule.audio_codecs.is_empty(),
        };
        let audio_ok = match audio {
            Some(a) => rule.audio_codecs.iter().any(|c| c.eq_ignore_ascii_case(&a.codec)),
            None => true,
        };
        video_ok && audio_ok
    });

    if can_direct_play_rule
        && video_reasons.is_empty()
        && audio_channel_reason.is_none()
        && bitrate_too_high.is_none()
        && subtitle != SubtitleDelivery::BurnIn
    {
        return Decision {
            method: PlayMethod::DirectPlay,
            container: canonical_container(&media.container, video),
            video: if video.is_some() { StreamAction::Copy } else { StreamAction::Drop },
            audio: if audio.is_some() { StreamAction::Copy } else { StreamAction::Drop },
            subtitle,
            reasons: Vec::new(),
        };
    }

    // 5. Non-direct play: collect reasons in deterministic order.
    let mut reasons = Vec::new();

    // Did the container itself prevent direct play for otherwise-supported codecs?
    if !can_direct_play_rule {
        let video_supported_somewhere = video.is_none_or(|v| {
            profile
                .direct_play
                .iter()
                .any(|r| r.video_codecs.iter().any(|c| c.eq_ignore_ascii_case(&v.codec)))
        });
        let container_supports_video = video.is_none_or(|v| {
            profile.direct_play.iter().any(|r| {
                container_matches(&media.container, &r.containers)
                    && r.video_codecs.iter().any(|c| c.eq_ignore_ascii_case(&v.codec))
            })
        });
        let any_rule_has_container =
            profile.direct_play.iter().any(|r| container_matches(&media.container, &r.containers));

        if !any_rule_has_container || (video_supported_somewhere && !container_supports_video) {
            reasons.push(Reason::ContainerUnsupported(media.container.clone()));
        }
    }

    reasons.extend(video_reasons.iter().cloned());

    if let Some((source, limit)) = bitrate_too_high {
        reasons.push(Reason::BitrateTooHigh { source, limit });
    }

    // Determine whether audio can be copied in the HLS target container.
    let audio_supported_in_target = audio.is_none_or(|a| {
        profile.transcode_target.audio_codecs.iter().any(|c| c.eq_ignore_ascii_case(&a.codec))
    });
    let audio_supported_in_dp = audio.is_none_or(|a| {
        profile
            .direct_play
            .iter()
            .any(|r| r.audio_codecs.iter().any(|c| c.eq_ignore_ascii_case(&a.codec)))
    });

    if let Some(a) = audio
        && (!audio_supported_in_target || !audio_supported_in_dp)
    {
        reasons.push(Reason::AudioCodecUnsupported(a.codec.clone()));
    }
    if let Some(r) = audio_channel_reason.clone() {
        reasons.push(r);
    }

    if subtitle == SubtitleDelivery::BurnIn
        && let Some(sub) = subtitle_stream
    {
        reasons.push(Reason::SubtitleBurnInRequired(sub.codec.clone()));
    }

    // 6. Decide video action (Copy vs Transcode vs Drop)
    let video_in_target = video.is_some_and(|v| {
        profile.transcode_target.video_codecs.iter().any(|c| c.eq_ignore_ascii_case(&v.codec))
    });
    let must_transcode_video = video.is_some()
        && (!video_reasons.is_empty()
            || !video_in_target
            || bitrate_too_high.is_some()
            || subtitle == SubtitleDelivery::BurnIn);

    let video_action = match video {
        None => StreamAction::Drop,
        Some(_) if !must_transcode_video => StreamAction::Copy,
        Some(v) => {
            if !ctx.can_transcode_video && ctx.hw.is_none() {
                reasons.push(Reason::NoTranscodeCapacity);
            }
            let target_codec = pick_video_transcode_codec(profile, ctx);
            let width = pick_video_width(v, profile, &target_codec, effective_max_bitrate);
            let bitrate = pick_video_bitrate(v, width, effective_max_bitrate);
            StreamAction::Transcode { codec: target_codec, bitrate, width }
        }
    };

    // 7. Decide audio action (Copy vs Transcode vs Drop)
    let must_transcode_audio = audio.is_some()
        && (!audio_supported_in_target
            || audio_channel_reason.is_some()
            || (video.is_none() && bitrate_too_high.is_some()));

    let audio_action = match audio {
        None => StreamAction::Drop,
        Some(_) if !must_transcode_audio => StreamAction::Copy,
        Some(a) => {
            let codec = profile
                .transcode_target
                .audio_codecs
                .first()
                .cloned()
                .unwrap_or_else(|| "aac".into());
            let channels = a.channels.unwrap_or(2).min(profile.max_audio_channels).max(1);
            let default_br = match channels {
                1 => 128_000,
                2 => 192_000,
                _ => 384_000,
            };
            let bitrate = match (video.is_none(), effective_max_bitrate) {
                (true, Some(limit)) => default_br.min(limit),
                _ => default_br,
            };
            StreamAction::Transcode { codec, bitrate, width: None }
        }
    };

    let method = if matches!(video_action, StreamAction::Transcode { .. })
        || matches!(audio_action, StreamAction::Transcode { .. })
    {
        PlayMethod::Transcode
    } else {
        PlayMethod::DirectStream
    };

    Decision {
        method,
        container: profile.transcode_target.container.clone(),
        video: video_action,
        audio: audio_action,
        subtitle,
        reasons,
    }
}

fn select_audio(media: &MediaInfo, requested: Option<u32>) -> Option<&StreamInfo> {
    if let Some(idx) = requested
        && let Some(found) = media.streams_of(StreamKind::Audio).find(|s| s.index == idx)
    {
        return Some(found);
    }
    let mut audios = media.streams_of(StreamKind::Audio);
    let first = audios.next()?;
    Some(std::iter::once(first).chain(audios).find(|s| s.default).unwrap_or(first))
}

fn evaluate_subtitle(sub: &StreamInfo, profile: &DeviceProfile) -> SubtitleDelivery {
    if let Some(rule) = profile.subtitle_rule(&sub.codec) {
        return match rule.method {
            SubtitleMethod::External => SubtitleDelivery::Sidecar {
                format: rule.target_format.clone().unwrap_or_else(|| "vtt".into()),
            },
            SubtitleMethod::Embed => SubtitleDelivery::Embedded,
            SubtitleMethod::Burn => SubtitleDelivery::BurnIn,
        };
    }
    if matches!(
        sub.codec.to_ascii_lowercase().as_str(),
        "subrip" | "srt" | "ass" | "ssa" | "webvtt" | "vtt" | "mov_text" | "text"
    ) {
        SubtitleDelivery::Sidecar { format: "vtt".into() }
    } else {
        SubtitleDelivery::BurnIn
    }
}

fn evaluate_video(v: &StreamInfo, profile: &DeviceProfile, reasons: &mut Vec<Reason>) {
    let Some(limit) = profile.codec_limit(&v.codec) else {
        reasons.push(Reason::VideoCodecUnsupported(v.codec.clone()));
        if v.interlaced {
            reasons.push(Reason::Other(format!("Video stream '{}' is interlaced", v.codec)));
        }
        return;
    };

    if !limit.profiles.is_empty()
        && let Some(prof) = &v.profile
        && !limit.profiles.iter().any(|p| p.eq_ignore_ascii_case(prof))
    {
        reasons.push(Reason::VideoProfileUnsupported(format!("{} {}", v.codec, prof)));
    }

    if let (Some(max_level), Some(level)) = (limit.max_level, v.level)
        && level > max_level
    {
        reasons.push(Reason::VideoProfileUnsupported(format!(
            "{} level {} exceeds max {}",
            v.codec, level, max_level
        )));
    }

    if let (Some(max_bd), Some(bd)) = (limit.max_bit_depth, v.bit_depth)
        && bd > max_bd
    {
        reasons.push(Reason::BitDepthUnsupported(bd));
    }

    if let Some(hdr) = v.hdr
        && !hdr_supported(limit, hdr)
    {
        reasons.push(Reason::HdrUnsupported);
    }

    let width_exceeded = limit.max_width.is_some_and(|mw| v.width.unwrap_or(0) > mw);
    let height_exceeded = limit.max_height.is_some_and(|mh| v.height.unwrap_or(0) > mh);
    if width_exceeded || height_exceeded {
        reasons.push(Reason::ResolutionTooHigh);
    }

    if v.interlaced {
        reasons.push(Reason::Other(format!("Video stream '{}' is interlaced", v.codec)));
    }
}

fn hdr_supported(limit: &CodecLimit, hdr: HdrKind) -> bool {
    if !limit.hdr {
        return false;
    }
    if limit.hdr_formats.is_empty() {
        return match hdr {
            HdrKind::Hdr10 | HdrKind::Hdr10Plus | HdrKind::Hlg => true,
            // Per PLAN.md §6.5, DV Profile 8 (or compat_id 1) falls back to its HDR10 base layer,
            // whereas DV Profile 5 has no HDR10 base layer and requires tone mapping.
            HdrKind::DolbyVision { profile, compat_id } => {
                profile != 5 && (profile == 8 || compat_id == 1)
            }
        };
    }
    let has = |name: &str| limit.hdr_formats.iter().any(|f| f.eq_ignore_ascii_case(name));
    match hdr {
        HdrKind::Hdr10 => has("hdr10"),
        HdrKind::Hdr10Plus => has("hdr10plus") || has("hdr10"),
        HdrKind::Hlg => has("hlg"),
        HdrKind::DolbyVision { profile, compat_id } => {
            has("dv")
                || has("dolbyvision")
                || (profile != 5 && (profile == 8 || compat_id == 1) && has("hdr10"))
        }
    }
}

fn container_matches(media_container: &str, rule_containers: &[String]) -> bool {
    media_container
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .any(|tok| rule_containers.iter().any(|rc| container_token_eq(tok, rc)))
}

fn container_token_eq(a: &str, b: &str) -> bool {
    let norm = |s: &str| match s.trim().to_ascii_lowercase().as_str() {
        "mkv" => "matroska".to_owned(),
        other => other.to_owned(),
    };
    norm(a) == norm(b)
}

fn canonical_container(media_container: &str, video: Option<&StreamInfo>) -> String {
    let tokens: Vec<&str> = media_container.split(',').map(str::trim).collect();
    if tokens.contains(&"mp4") {
        return "mp4".into();
    }
    if tokens.contains(&"webm")
        && video.is_some_and(|v| matches!(v.codec.as_str(), "vp8" | "vp9" | "av1"))
    {
        return "webm".into();
    }
    tokens.first().copied().unwrap_or(media_container).to_owned()
}

fn pick_video_transcode_codec(profile: &DeviceProfile, ctx: &DecisionContext) -> String {
    if let Some(hw) = &ctx.hw {
        for wanted in &profile.transcode_target.video_codecs {
            if hw.encode_codecs.iter().any(|c| c.eq_ignore_ascii_case(wanted)) {
                return wanted.clone();
            }
        }
    }
    // Software fallback is always H.264 if accepted by the target, otherwise the first target codec.
    if profile.transcode_target.video_codecs.iter().any(|c| c.eq_ignore_ascii_case("h264")) {
        "h264".into()
    } else {
        profile.transcode_target.video_codecs.first().cloned().unwrap_or_else(|| "h264".into())
    }
}

fn pick_video_width(
    v: &StreamInfo,
    profile: &DeviceProfile,
    target_codec: &str,
    max_bitrate: Option<u64>,
) -> Option<u32> {
    let src_w = v.width?;
    let mut target_w = src_w;

    if let Some(limit) = profile.codec_limit(target_codec)
        && let Some(max_w) = limit.max_width
    {
        target_w = target_w.min(max_w);
    }

    if let Some(limit_br) = max_bitrate {
        if limit_br < 1_000_000 {
            target_w = target_w.min(854);
        } else if limit_br < 2_500_000 {
            target_w = target_w.min(1280);
        } else if limit_br < 6_000_000 {
            target_w = target_w.min(1920);
        }
    }

    (target_w < src_w).then_some(target_w)
}

fn pick_video_bitrate(v: &StreamInfo, scaled_width: Option<u32>, max_bitrate: Option<u64>) -> u64 {
    let effective_w = scaled_width.or(v.width).unwrap_or(1920);
    let base: u64 = if effective_w >= 3840 {
        20_000_000
    } else if effective_w >= 1920 {
        8_000_000
    } else if effective_w >= 1280 {
        4_000_000
    } else {
        2_000_000
    };
    match max_bitrate {
        Some(limit) => base.min(limit),
        None => base,
    }
}
