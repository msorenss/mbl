//! Home Assistant MQTT Integration (M7.15).
//! Generates MQTT Discovery payloads for media players and diagnostic sensors,
//! formats playback state and lifecycle trigger events, and parses control commands.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub enabled: bool,
    pub broker_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub topic_prefix: String,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            broker_url: "mqtt://localhost:1883".into(),
            username: None,
            password: None,
            topic_prefix: "mbl".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HassDeviceInfo {
    pub identifiers: Vec<String>,
    pub name: String,
    pub manufacturer: String,
    pub model: String,
    pub sw_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPlayerDiscovery {
    pub name: String,
    pub unique_id: String,
    pub state_topic: String,
    pub command_topic: String,
    pub json_attributes_topic: String,
    pub supported_features: u32,
    pub device: HassDeviceInfo,
}

/// Home Assistant media_player supported features bitflags
pub const SUPPORT_PAUSE: u32 = 1;
pub const SUPPORT_SEEK: u32 = 2;
pub const SUPPORT_VOLUME_SET: u32 = 4;
pub const SUPPORT_VOLUME_MUTE: u32 = 8;
pub const SUPPORT_PREVIOUS_TRACK: u32 = 16;
pub const SUPPORT_NEXT_TRACK: u32 = 32;
pub const SUPPORT_PLAY: u32 = 128;
pub const SUPPORT_STOP: u32 = 256;

/// Builds the Home Assistant MQTT Discovery topic and JSON payload for a player.
pub fn build_player_discovery(
    topic_prefix: &str,
    player_id: &str,
    player_name: &str,
    server_version: &str,
) -> (String, String) {
    let discovery_topic = format!("homeassistant/media_player/{topic_prefix}_{player_id}/config");
    let state_topic = format!("{topic_prefix}/player/{player_id}/state");
    let command_topic = format!("{topic_prefix}/player/{player_id}/command");
    let attrs_topic = format!("{topic_prefix}/player/{player_id}/attributes");

    let payload = MediaPlayerDiscovery {
        name: format!("MBL {}", player_name),
        unique_id: format!("{topic_prefix}_{player_id}"),
        state_topic,
        command_topic,
        json_attributes_topic: attrs_topic,
        supported_features: SUPPORT_PLAY
            | SUPPORT_PAUSE
            | SUPPORT_STOP
            | SUPPORT_SEEK
            | SUPPORT_VOLUME_SET,
        device: HassDeviceInfo {
            identifiers: vec![format!("{topic_prefix}_{player_id}")],
            name: format!("MBL Player ({player_name})"),
            manufacturer: "MBL".into(),
            model: "Media Player Client".into(),
            sw_version: server_version.into(),
        },
    };

    let json = serde_json::to_string(&payload).unwrap_or_default();
    (discovery_topic, json)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorDiscovery {
    pub name: String,
    pub unique_id: String,
    pub state_topic: String,
    pub icon: String,
    pub device: HassDeviceInfo,
}

/// Builds Home Assistant sensor discovery for diagnostic sensors (CPU, thermal throttle, transcode count).
pub fn build_sensor_discovery(
    topic_prefix: &str,
    sensor_id: &str,
    sensor_name: &str,
    icon: &str,
    server_version: &str,
) -> (String, String) {
    let discovery_topic = format!("homeassistant/sensor/{topic_prefix}_{sensor_id}/config");
    let state_topic = format!("{topic_prefix}/sensor/{sensor_id}/state");

    let payload = SensorDiscovery {
        name: format!("MBL {}", sensor_name),
        unique_id: format!("{topic_prefix}_{sensor_id}"),
        state_topic,
        icon: icon.into(),
        device: HassDeviceInfo {
            identifiers: vec![format!("{topic_prefix}_server")],
            name: "MBL Media Server".into(),
            manufacturer: "MBL".into(),
            model: "Server Node".into(),
            sw_version: server_version.into(),
        },
    };

    let json = serde_json::to_string(&payload).unwrap_or_default();
    (discovery_topic, json)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStatePayload {
    pub state: String, // "playing", "paused", "idle"
    pub media_title: Option<String>,
    pub media_artist: Option<String>,
    pub media_series: Option<String>,
    pub media_episode: Option<i64>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub volume_level: f64, // 0.0 .. 1.0
    pub playback_method: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncomingPlayerCommand {
    Play,
    Pause,
    Stop,
    Seek(i64),
    SetVolume(u32),
    Unknown(String),
}

/// Parses raw text received on a player's command topic.
pub fn parse_incoming_command(msg: &str) -> IncomingPlayerCommand {
    let trimmed = msg.trim();
    if trimmed.eq_ignore_ascii_case("PLAY") {
        IncomingPlayerCommand::Play
    } else if trimmed.eq_ignore_ascii_case("PAUSE") {
        IncomingPlayerCommand::Pause
    } else if trimmed.eq_ignore_ascii_case("STOP") {
        IncomingPlayerCommand::Stop
    } else if let Some(seek_s) = trimmed.strip_prefix("SEEK ") {
        if let Ok(sec) = seek_s.trim().parse::<f64>() {
            IncomingPlayerCommand::Seek((sec * 1000.0) as i64)
        } else {
            IncomingPlayerCommand::Unknown(trimmed.to_owned())
        }
    } else if let Some(vol_s) = trimmed.strip_prefix("VOLUME ") {
        if let Ok(vol) = vol_s.trim().parse::<u32>() {
            IncomingPlayerCommand::SetVolume(vol.min(100))
        } else {
            IncomingPlayerCommand::Unknown(trimmed.to_owned())
        }
    } else {
        IncomingPlayerCommand::Unknown(trimmed.to_owned())
    }
}

/// Event topics for room lighting and cinema automation triggers.
pub fn playback_event_topic(topic_prefix: &str, event_type: &str) -> String {
    format!("{topic_prefix}/events/playback_{event_type}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_discovery_payload_generation() {
        let (topic, json) =
            build_player_discovery("mbl", "living_room_tv", "Living Room TV", "0.6.0");
        assert_eq!(topic, "homeassistant/media_player/mbl_living_room_tv/config");
        assert!(json.contains("Living Room TV"));
        assert!(json.contains("mbl/player/living_room_tv/state"));
        assert!(json.contains("mbl/player/living_room_tv/command"));
    }

    #[test]
    fn command_parsing() {
        assert_eq!(parse_incoming_command("PLAY"), IncomingPlayerCommand::Play);
        assert_eq!(parse_incoming_command("pause"), IncomingPlayerCommand::Pause);
        assert_eq!(parse_incoming_command("SEEK 45.5"), IncomingPlayerCommand::Seek(45500));
        assert_eq!(parse_incoming_command("VOLUME 75"), IncomingPlayerCommand::SetVolume(75));
    }
}
