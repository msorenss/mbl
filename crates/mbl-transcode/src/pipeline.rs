//! Typed FFmpeg transcode pipeline model (IMPLEMENTATION.md Appendix A.8).

use std::str::FromStr;

use camino::Utf8PathBuf;
use mbl_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Complete, typed specification of an FFmpeg transcode or remux job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    pub input: Utf8PathBuf,
    pub start_ms: u64,
    pub video: VideoOp,
    pub audio: AudioOp,
    pub subtitle: SubtitleOp,
    pub output: HlsOutput,
    pub backend: Backend,
}

/// Video stream operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum VideoOp {
    Copy,
    Encode {
        codec: VideoCodec,
        width: Option<u32>,
        max_bitrate: u64,
        deinterlace: bool,
        tonemap: bool,
    },
    None,
}

/// Target video codec for transcoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    Hevc,
    Av1,
}

impl VideoCodec {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
            Self::Av1 => "av1",
        }
    }
}

impl FromStr for VideoCodec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "h264" | "avc" | "libx264" => Ok(Self::H264),
            "hevc" | "h265" | "libx265" => Ok(Self::Hevc),
            "av1" | "libsvtav1" => Ok(Self::Av1),
            other => Err(Error::Validation(format!("unsupported video encode codec '{other}'"))),
        }
    }
}

/// Audio stream operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum AudioOp {
    None,
    Copy {
        index: u32,
    },
    Encode {
        index: u32,
        codec: AudioCodec,
        channels: u8,
        bitrate: u64,
    },
    /// Signed audio synchronization in milliseconds; positive delays, negative advances.
    EncodeShifted {
        index: u32,
        codec: AudioCodec,
        channels: u8,
        bitrate: u64,
        offset_ms: i64,
    },
}

/// Target audio codec for transcoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Aac,
    Ac3,
    Opus,
}

impl AudioCodec {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Ac3 => "ac3",
            Self::Opus => "opus",
        }
    }

    pub const fn ffmpeg_encoder(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Ac3 => "ac3",
            Self::Opus => "libopus",
        }
    }
}

impl FromStr for AudioCodec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "aac" => Ok(Self::Aac),
            "ac3" => Ok(Self::Ac3),
            "opus" | "libopus" => Ok(Self::Opus),
            other => Err(Error::Validation(format!("unsupported audio encode codec '{other}'"))),
        }
    }
}

/// Subtitle burn-in operation (`None` when subtitles are off or served as sidecar WebVTT/ASS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum SubtitleOp {
    None,
    /// Burn a text subtitle stream (`index` is the 0-based subtitle stream ordinal).
    BurnText {
        index: u32,
    },
    /// Burn a standalone, validated text subtitle file.
    BurnTextFile {
        path: Utf8PathBuf,
    },
    /// Burn a bitmap subtitle stream (`index` is the FFmpeg stream index in the input file).
    BurnImage {
        index: u32,
    },
}

/// HLS fMP4 output configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsOutput {
    pub dir: Utf8PathBuf,
    pub segment_secs: u32,
    pub start_number: u32,
    pub renditions: Vec<Rendition>,
}

impl HlsOutput {
    /// Target directory for the given rendition (or `self.dir` when `renditions` is empty).
    pub fn rendition_dir(&self, index: usize) -> Utf8PathBuf {
        match self.renditions.get(index) {
            Some(r) if !r.name.is_empty() => self.dir.join(&r.name),
            _ => self.dir.clone(),
        }
    }
}

/// One HLS rendition in an output ladder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendition {
    pub name: String,
    pub width: Option<u32>,
    pub max_bitrate: u64,
}

/// Hardware or software transcode backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Backend {
    Software,
    Vaapi,
    Qsv,
    Nvenc,
    V4l2m2m,
    PiDrmDecode,
    D3d11Amf,
}

impl Backend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Software => "software",
            Self::Vaapi => "vaapi",
            Self::Qsv => "qsv",
            Self::Nvenc => "nvenc",
            Self::V4l2m2m => "v4l2m2m",
            Self::PiDrmDecode => "piDrmDecode",
            Self::D3d11Amf => "d3d11Amf",
        }
    }

    /// True for full-GPU hardware encode backends that support multi-rendition ABR ladders (M4.13).
    pub const fn is_gpu(self) -> bool {
        matches!(self, Self::Vaapi | Self::Qsv | Self::Nvenc | Self::D3d11Amf)
    }

    /// True for Raspberry Pi specific backends where software tone mapping and ABR ladders are disabled.
    pub const fn is_pi(self) -> bool {
        matches!(self, Self::V4l2m2m | Self::PiDrmDecode)
    }
}

impl FromStr for Backend {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim() {
            "software" | "Software" => Ok(Self::Software),
            "vaapi" | "Vaapi" => Ok(Self::Vaapi),
            "qsv" | "Qsv" => Ok(Self::Qsv),
            "nvenc" | "Nvenc" | "cuda" => Ok(Self::Nvenc),
            "v4l2m2m" | "V4l2m2m" => Ok(Self::V4l2m2m),
            "piDrmDecode" | "PiDrmDecode" | "pi_drm_decode" => Ok(Self::PiDrmDecode),
            "d3d11Amf" | "D3d11Amf" | "d3d11_amf" | "amf" => Ok(Self::D3d11Amf),
            other => Err(Error::Validation(format!("unknown transcode backend '{other}'"))),
        }
    }
}

/// Encoder tuning and capabilities discovered for the active FFmpeg binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegCaps {
    /// x264/x265 preset (default `"veryfast"`).
    pub preset: String,
    /// Constant Rate Factor quality target (default `21`).
    pub crf: u8,
    /// Optional explicit thread count (`-threads`).
    pub threads: Option<u32>,
}

impl Default for FfmpegCaps {
    fn default() -> Self {
        Self { preset: "veryfast".to_owned(), crf: 21, threads: None }
    }
}
