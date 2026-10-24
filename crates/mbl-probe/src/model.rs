use mbl_core::StreamKind;
use serde::{Deserialize, Serialize};

/// Normalized result of probing one media file. Stored as JSON in `media_parts.probe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub container: String,
    pub duration_ms: u64,
    pub bitrate: Option<u64>,
    pub size: u64,
    pub streams: Vec<StreamInfo>,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub index: u32,
    pub kind: StreamKind,
    pub codec: String,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    /// Set for sidecar subtitle files found next to the media file.
    pub external_path: Option<String>,
    // video
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bit_depth: Option<u8>,
    pub frame_rate: Option<f32>,
    pub interlaced: bool,
    pub hdr: Option<HdrKind>,
    // audio
    pub channels: Option<u8>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HdrKind {
    Hdr10,
    Hdr10Plus,
    Hlg,
    DolbyVision { profile: u8, compat_id: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: Option<String>,
}

impl MediaInfo {
    pub fn streams_of(&self, kind: StreamKind) -> impl Iterator<Item = &StreamInfo> {
        self.streams.iter().filter(move |s| s.kind == kind)
    }

    /// The default video stream, or the first one.
    pub fn primary_video(&self) -> Option<&StreamInfo> {
        let mut videos = self.streams_of(StreamKind::Video);
        let first = videos.next()?;
        Some(
            std::iter::once(first)
                .chain(videos)
                .find(|s| s.default)
                .unwrap_or(first),
        )
    }
}
