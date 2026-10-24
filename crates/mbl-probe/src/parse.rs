//! Mapping from raw `ffprobe -of json` output to [`MediaInfo`].

use std::collections::HashMap;

use mbl_core::{Error, Result, StreamKind};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{Chapter, HdrKind, MediaInfo, StreamInfo};

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    streams: Vec<RawStream>,
    #[serde(default)]
    chapters: Vec<RawChapter>,
    format: Option<RawFormat>,
}

#[derive(Deserialize)]
struct RawFormat {
    format_name: Option<String>,
    duration: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Deserialize)]
struct RawChapter {
    start_time: Option<String>,
    end_time: Option<String>,
    #[serde(default)]
    tags: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct RawStream {
    index: u32,
    codec_type: Option<String>,
    codec_name: Option<String>,
    profile: Option<String>,
    level: Option<i32>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    bits_per_raw_sample: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    field_order: Option<String>,
    color_transfer: Option<String>,
    channels: Option<u8>,
    channel_layout: Option<String>,
    sample_rate: Option<String>,
    #[serde(default)]
    disposition: HashMap<String, i64>,
    #[serde(default)]
    tags: HashMap<String, Value>,
    #[serde(default)]
    side_data_list: Vec<Value>,
}

fn tag(tags: &HashMap<String, Value>, key: &str) -> Option<String> {
    tags.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

fn secs_to_ms(s: Option<&str>) -> Option<u64> {
    let secs: f64 = s?.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as u64)
}

fn parse_rate(s: Option<&str>) -> Option<f32> {
    let (n, d) = s?.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0.0 && d > 0.0).then(|| ((n / d) * 1000.0).round() as f32 / 1000.0)
}

fn bit_depth(raw: &RawStream) -> Option<u8> {
    if let Some(b) = raw
        .bits_per_raw_sample
        .as_deref()
        .and_then(|b| b.parse::<u8>().ok())
    {
        return Some(b);
    }
    let pix = raw.pix_fmt.as_deref()?;
    Some(if pix.contains("12") {
        12
    } else if pix.contains("10") {
        10
    } else {
        8
    })
}

fn hdr(raw: &RawStream) -> Option<HdrKind> {
    let side_type = |v: &Value| {
        v.get("side_data_type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    if let Some(dv) = raw
        .side_data_list
        .iter()
        .find(|v| side_type(v).contains("DOVI"))
    {
        let field = |k: &str| {
            dv.get(k)
                .and_then(Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
        };
        return Some(HdrKind::DolbyVision {
            profile: field("dv_profile").unwrap_or(0),
            compat_id: field("dv_bl_signal_compatibility_id").unwrap_or(0),
        });
    }
    match raw.color_transfer.as_deref() {
        Some("smpte2084") => {
            let plus = raw
                .side_data_list
                .iter()
                .any(|v| side_type(v).contains("2094-40"));
            Some(if plus {
                HdrKind::Hdr10Plus
            } else {
                HdrKind::Hdr10
            })
        }
        Some("arib-std-b67") => Some(HdrKind::Hlg),
        _ => None,
    }
}

fn stream(raw: RawStream) -> Option<StreamInfo> {
    let attached_pic = raw.disposition.get("attached_pic") == Some(&1);
    let kind = match raw.codec_type.as_deref()? {
        "video" if attached_pic => StreamKind::Attachment,
        "video" => StreamKind::Video,
        "audio" => StreamKind::Audio,
        "subtitle" => StreamKind::Subtitle,
        "attachment" => StreamKind::Attachment,
        _ => return None,
    };
    let is_video = kind == StreamKind::Video;
    let is_audio = kind == StreamKind::Audio;
    Some(StreamInfo {
        index: raw.index,
        kind,
        codec: raw.codec_name.clone().unwrap_or_else(|| "unknown".into()),
        profile: raw.profile.clone(),
        level: raw.level.filter(|l| is_video && *l > 0),
        language: tag(&raw.tags, "language").filter(|l| l != "und"),
        title: tag(&raw.tags, "title"),
        default: raw.disposition.get("default") == Some(&1),
        forced: raw.disposition.get("forced") == Some(&1),
        external_path: None,
        width: raw.width.filter(|_| is_video),
        height: raw.height.filter(|_| is_video),
        bit_depth: if is_video { bit_depth(&raw) } else { None },
        frame_rate: if is_video {
            parse_rate(raw.avg_frame_rate.as_deref())
                .or_else(|| parse_rate(raw.r_frame_rate.as_deref()))
        } else {
            None
        },
        interlaced: is_video
            && matches!(raw.field_order.as_deref(), Some("tt" | "bb" | "tb" | "bt")),
        hdr: if is_video { hdr(&raw) } else { None },
        channels: raw.channels.filter(|_| is_audio),
        channel_layout: raw.channel_layout.filter(|_| is_audio),
        sample_rate: raw
            .sample_rate
            .as_deref()
            .and_then(|s| s.parse().ok())
            .filter(|_| is_audio),
    })
}

/// Parse `ffprobe -show_format -show_streams -show_chapters -of json` output.
pub fn parse_ffprobe_json(bytes: &[u8]) -> Result<MediaInfo> {
    let raw: Raw = serde_json::from_slice(bytes)
        .map_err(|e| Error::Validation(format!("unreadable ffprobe output: {e}")))?;
    let format = raw
        .format
        .ok_or_else(|| Error::Validation("ffprobe output has no format section".into()))?;
    let chapters = raw
        .chapters
        .into_iter()
        .filter_map(|c| {
            Some(Chapter {
                start_ms: secs_to_ms(c.start_time.as_deref())?,
                end_ms: secs_to_ms(c.end_time.as_deref())?,
                title: tag(&c.tags, "title"),
            })
        })
        .collect();
    Ok(MediaInfo {
        container: format.format_name.unwrap_or_default(),
        duration_ms: secs_to_ms(format.duration.as_deref()).unwrap_or(0),
        bitrate: format.bit_rate.as_deref().and_then(|b| b.parse().ok()),
        size: format
            .size
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        streams: raw.streams.into_iter().filter_map(stream).collect(),
        chapters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_and_times() {
        assert_eq!(parse_rate(Some("24000/1001")), Some(23.976));
        assert_eq!(parse_rate(Some("0/0")), None);
        assert_eq!(secs_to_ms(Some("7380.5")), Some(7_380_500));
        assert_eq!(secs_to_ms(Some("N/A")), None);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_ffprobe_json(b"not json").is_err());
        assert!(parse_ffprobe_json(b"{}").is_err());
    }
}
