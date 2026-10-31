//! Public web discovery metadata. A search hit is not a verified playback source.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnlineAvailability {
    Unchecked,
    ExternalPage,
    ExternalPlayer,
    MediaDetected,
    Restricted,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlineEpisode {
    pub title: String,
    pub url: String,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlineSource {
    pub id: String,
    pub url: String,
    pub title: String,
    pub summary: Option<String>,
    pub kind: String,
    pub series_title: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub episodes: Vec<OnlineEpisode>,
    pub availability: OnlineAvailability,
    pub checked_at: Option<i64>,
    pub media_url: Option<String>,
    pub has_video: Option<bool>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlineSearchResponse {
    pub query: String,
    pub provider: String,
    pub results: Vec<OnlineSource>,
    pub indexed: Vec<OnlineSource>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlinePlayback {
    pub id: String,
    pub stream_url: String,
    pub hls: bool,
    pub position: f64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlineProgress {
    pub position: f64,
    pub duration: f64,
    #[serde(default)]
    pub ended: bool,
    #[serde(default)]
    pub heartbeat_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlineHistoryEntry {
    pub source: OnlineSource,
    pub position: f64,
    pub duration: f64,
    pub updated_at: i64,
}
