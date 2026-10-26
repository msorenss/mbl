//! Live TV and DVR domain types (M9.1 - M9.5).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::{ChannelId, ProgramId, RecordingId, RecordingRuleId, TunerId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TunerKind {
    Hdhomerun,
    M3u,
}

impl TunerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hdhomerun => "hdhomerun",
            Self::M3u => "m3u",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hdhomerun" => Some(Self::Hdhomerun),
            "m3u" => Some(Self::M3u),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TunerStatus {
    Online,
    Offline,
    Scanning,
}

impl TunerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Scanning => "scanning",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "online" => Some(Self::Online),
            "offline" => Some(Self::Offline),
            "scanning" => Some(Self::Scanning),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Tuner {
    pub id: TunerId,
    pub name: String,
    pub kind: TunerKind,
    pub url: String,
    pub status: TunerStatus,
    #[serde(rename = "tunerCount")]
    pub tuner_count: u32,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTunerRequest {
    pub name: String,
    pub kind: TunerKind,
    pub url: String,
    #[serde(rename = "tunerCount", default)]
    pub tuner_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Channel {
    pub id: ChannelId,
    #[serde(rename = "tunerId", skip_serializing_if = "Option::is_none")]
    pub tuner_id: Option<TunerId>,
    #[serde(rename = "channelNumber")]
    pub channel_number: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callsign: Option<String>,
    #[serde(rename = "logoUrl", skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(rename = "streamUrl")]
    pub stream_url: String,
    #[serde(rename = "epgChannelId", skip_serializing_if = "Option::is_none")]
    pub epg_channel_id: Option<String>,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    #[serde(rename = "isHidden")]
    pub is_hidden: bool,
    #[serde(rename = "isVirtual")]
    pub is_virtual: bool,
    #[serde(rename = "currentProgram", skip_serializing_if = "Option::is_none")]
    pub current_program: Option<EpgProgram>,
    #[serde(rename = "nextProgram", skip_serializing_if = "Option::is_none")]
    pub next_program: Option<EpgProgram>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    #[serde(rename = "channelNumber")]
    pub channel_number: Option<String>,
    #[serde(rename = "epgChannelId")]
    pub epg_channel_id: Option<String>,
    #[serde(rename = "isFavorite")]
    pub is_favorite: Option<bool>,
    #[serde(rename = "isHidden")]
    pub is_hidden: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EpgProgram {
    pub id: ProgramId,
    #[serde(rename = "channelId", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(rename = "epgChannelId")]
    pub epg_channel_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "seasonNumber", skip_serializing_if = "Option::is_none")]
    pub season_number: Option<u32>,
    #[serde(rename = "episodeNumber", skip_serializing_if = "Option::is_none")]
    pub episode_number: Option<u32>,
    #[serde(rename = "startTime")]
    pub start_time: i64,
    #[serde(rename = "endTime")]
    pub end_time: i64,
    #[serde(rename = "iconUrl", skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(rename = "isNew")]
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EpgGridChannel {
    pub channel: Channel,
    pub programs: Vec<EpgProgram>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EpgGridResponse {
    #[serde(rename = "startTime")]
    pub start_time: i64,
    #[serde(rename = "endTime")]
    pub end_time: i64,
    pub channels: Vec<EpgGridChannel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecordingRule {
    pub id: RecordingRuleId,
    #[serde(rename = "userId")]
    pub user_id: UserId,
    pub title: String,
    #[serde(rename = "channelId", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    #[serde(rename = "seriesOnly")]
    pub series_only: bool,
    #[serde(rename = "seasonNumber", skip_serializing_if = "Option::is_none")]
    pub season_number: Option<u32>,
    #[serde(rename = "newOnly")]
    pub new_only: bool,
    #[serde(rename = "startPaddingSecs")]
    pub start_padding_secs: u32,
    #[serde(rename = "endPaddingSecs")]
    pub end_padding_secs: u32,
    #[serde(rename = "keepMax", skip_serializing_if = "Option::is_none")]
    pub keep_max: Option<u32>,
    #[serde(rename = "autoDeleteWatched")]
    pub auto_delete_watched: bool,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateRecordingRuleRequest {
    pub title: String,
    #[serde(rename = "channelId", default)]
    pub channel_id: Option<ChannelId>,
    #[serde(rename = "seriesOnly", default)]
    pub series_only: bool,
    #[serde(rename = "seasonNumber", default)]
    pub season_number: Option<u32>,
    #[serde(rename = "newOnly", default)]
    pub new_only: bool,
    #[serde(rename = "startPaddingSecs", default)]
    pub start_padding_secs: Option<u32>,
    #[serde(rename = "endPaddingSecs", default)]
    pub end_padding_secs: Option<u32>,
    #[serde(rename = "keepMax", default)]
    pub keep_max: Option<u32>,
    #[serde(rename = "autoDeleteWatched", default)]
    pub auto_delete_watched: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RecordingStatus {
    Scheduled,
    Recording,
    Completed,
    Failed,
    Cancelled,
}

impl RecordingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Recording => "recording",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "scheduled" => Some(Self::Scheduled),
            "recording" => Some(Self::Recording),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Recording {
    pub id: RecordingId,
    #[serde(rename = "ruleId", skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<RecordingRuleId>,
    #[serde(rename = "channelId")]
    pub channel_id: ChannelId,
    #[serde(rename = "channelName", skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(rename = "programId", skip_serializing_if = "Option::is_none")]
    pub program_id: Option<ProgramId>,
    pub title: String,
    #[serde(rename = "startTime")]
    pub start_time: i64,
    #[serde(rename = "endTime")]
    pub end_time: i64,
    pub status: RecordingStatus,
    #[serde(rename = "outputPath", skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(rename = "sizeBytes", skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(rename = "errorMessage", skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScheduleRecordingRequest {
    #[serde(rename = "channelId")]
    pub channel_id: ChannelId,
    #[serde(rename = "programId", default)]
    pub program_id: Option<ProgramId>,
    pub title: String,
    #[serde(rename = "startTime")]
    pub start_time: i64,
    #[serde(rename = "endTime")]
    pub end_time: i64,
    #[serde(rename = "startPaddingSecs", default)]
    pub start_padding_secs: Option<u32>,
    #[serde(rename = "endPaddingSecs", default)]
    pub end_padding_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LiveStreamInfo {
    #[serde(rename = "channelId")]
    pub channel_id: ChannelId,
    #[serde(rename = "channelName")]
    pub channel_name: String,
    #[serde(rename = "streamUrl")]
    pub stream_url: String,
    #[serde(rename = "isDeinterlacingRequired")]
    pub is_deinterlacing_required: bool,
    #[serde(rename = "targetFormat")]
    pub target_format: String,
}
