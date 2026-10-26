//! Online media plugins and virtual 24/7 channels domain types (M9.6 & M9.7).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::{ChannelId, ItemId, PluginId, VirtualChannelId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    LiveTvChannel,
    MediaScraper,
    AudioSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub capabilities: Vec<PluginCapability>,
    #[serde(rename = "iconUrl", skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PluginConfig {
    pub id: PluginId,
    #[serde(rename = "pluginId")]
    pub plugin_id: String,
    pub name: String,
    pub enabled: bool,
    pub settings: serde_json::Value,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PluginSubtitle {
    pub language: String,
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PluginItem {
    pub id: String,
    #[serde(rename = "pluginId")]
    pub plugin_id: String,
    pub title: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(rename = "thumbnailUrl", skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(rename = "streamUrl", skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
    #[serde(default)]
    pub subtitles: Vec<PluginSubtitle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PluginChannelFeed {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub items: Vec<PluginItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VirtualChannelMode {
    LinearLoop,
    Shuffle,
    SmartFilter,
}

impl VirtualChannelMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LinearLoop => "linear_loop",
            Self::Shuffle => "shuffle",
            Self::SmartFilter => "smart_filter",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "linear_loop" => Some(Self::LinearLoop),
            "shuffle" => Some(Self::Shuffle),
            "smart_filter" => Some(Self::SmartFilter),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VirtualChannelItem {
    #[serde(rename = "itemId")]
    pub item_id: ItemId,
    pub title: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    #[serde(rename = "mediaPath")]
    pub media_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VirtualChannel {
    pub id: VirtualChannelId,
    #[serde(rename = "channelId")]
    pub channel_id: ChannelId,
    pub name: String,
    pub mode: VirtualChannelMode,
    pub items: Vec<VirtualChannelItem>,
    #[serde(rename = "insertCommercials")]
    pub insert_commercials: bool,
    #[serde(rename = "totalDurationMs")]
    pub total_duration_ms: u64,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateVirtualChannelRequest {
    pub name: String,
    #[serde(rename = "channelNumber")]
    pub channel_number: String,
    pub mode: VirtualChannelMode,
    #[serde(rename = "itemIds")]
    pub item_ids: Vec<ItemId>,
    #[serde(rename = "insertCommercials", default)]
    pub insert_commercials: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VirtualPlayhead {
    #[serde(rename = "channelId")]
    pub channel_id: ChannelId,
    #[serde(rename = "channelName")]
    pub channel_name: String,
    #[serde(rename = "currentItemId")]
    pub current_item_id: ItemId,
    #[serde(rename = "currentTitle")]
    pub current_title: String,
    #[serde(rename = "mediaPath")]
    pub media_path: String,
    #[serde(rename = "offsetSecs")]
    pub offset_secs: f64,
    #[serde(rename = "durationSecs")]
    pub duration_secs: f64,
    #[serde(rename = "nextTitle", skip_serializing_if = "Option::is_none")]
    pub next_title: Option<String>,
    #[serde(rename = "calculatedAt")]
    pub calculated_at: i64,
}
