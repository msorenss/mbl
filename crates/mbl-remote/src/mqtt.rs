//! Home Assistant MQTT Integration (M7.15).
//! Generates MQTT Discovery payloads for media players and diagnostic sensors,
//! formats playback state and lifecycle trigger events, and parses control commands.

use rumqttc::{AsyncClient, EventLoop, LastWill, MqttOptions, QoS};
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

/// Parses an MQTT broker URL into `(host, port, is_tls)`.
/// Supports `mqtt://host:port`, `mqtts://host:port`, `ssl://host:port`, `tcp://host:port`,
/// and bare `host:port` or `host`.
pub fn parse_broker_url(url: &str) -> Result<(String, u16, bool), String> {
    let s = url.trim();
    let (is_tls, rest) = if let Some(stripped) = s.strip_prefix("mqtts://") {
        (true, stripped)
    } else if let Some(stripped) = s.strip_prefix("ssl://") {
        (true, stripped)
    } else if let Some(stripped) = s.strip_prefix("mqtt://") {
        (false, stripped)
    } else if let Some(stripped) = s.strip_prefix("tcp://") {
        (false, stripped)
    } else {
        (false, s)
    };

    let rest = rest.trim_end_matches('/');
    let (host, port) = if let Some((h, p)) = rest.split_once(':') {
        let port: u16 = p.parse().map_err(|e| format!("invalid MQTT port: {e}"))?;
        (h.to_string(), port)
    } else {
        let default_port = if is_tls { 8883 } else { 1883 };
        (rest.to_string(), default_port)
    };

    if host.is_empty() {
        return Err("empty MQTT host".into());
    }

    Ok((host, port, is_tls))
}

/// Creates a new `(AsyncClient, EventLoop)` from the given `MqttConfig`.
pub fn create_mqtt_client(
    config: &MqttConfig,
    client_suffix: &str,
) -> Result<(AsyncClient, EventLoop), String> {
    let (host, port, is_tls) = parse_broker_url(&config.broker_url)?;
    let client_id = format!("{}_{}_{}", config.topic_prefix, client_suffix, std::process::id());
    let mut options = MqttOptions::new(client_id, host, port);
    options.set_keep_alive(std::time::Duration::from_secs(15));
    if let (Some(u), Some(p)) = (&config.username, &config.password)
        && !u.trim().is_empty()
    {
        options.set_credentials(u.trim(), p);
    }
    let status_topic = format!("{}/status", config.topic_prefix);
    let will = LastWill::new(&status_topic, "offline", QoS::AtLeastOnce, true);
    options.set_last_will(will);

    if is_tls {
        return Err("TLS for MQTT broker is not supported; connect via LAN TCP or Tailscale".into());
    }

    let (client, eventloop) = AsyncClient::new(options, 30);
    Ok((client, eventloop))
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_of_measurement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_class: Option<String>,
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
        unit_of_measurement: None,
        state_class: None,
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

/// Builds Home Assistant measurement sensor discovery with units.
pub fn build_measurement_sensor_discovery(
    topic_prefix: &str,
    sensor_id: &str,
    sensor_name: &str,
    icon: &str,
    unit: &str,
    server_version: &str,
) -> (String, String) {
    let discovery_topic = format!("homeassistant/sensor/{topic_prefix}_{sensor_id}/config");
    let state_topic = format!("{topic_prefix}/sensor/{sensor_id}/state");

    let payload = SensorDiscovery {
        name: format!("MBL {}", sensor_name),
        unique_id: format!("{topic_prefix}_{sensor_id}"),
        state_topic,
        icon: icon.into(),
        unit_of_measurement: Some(unit.into()),
        state_class: Some("measurement".into()),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    if trimmed.eq_ignore_ascii_case("PLAY") || trimmed.eq_ignore_ascii_case("RESUME") {
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
        let vol_str = vol_s.trim();
        if let Ok(vol) = vol_str.parse::<u32>() {
            IncomingPlayerCommand::SetVolume(vol.min(100))
        } else if let Ok(vol_f) = vol_str.parse::<f64>() {
            if (0.0..=1.0).contains(&vol_f) {
                IncomingPlayerCommand::SetVolume((vol_f * 100.0).round() as u32)
            } else {
                IncomingPlayerCommand::SetVolume(vol_f.clamp(0.0, 100.0).round() as u32)
            }
        } else {
            IncomingPlayerCommand::Unknown(trimmed.to_owned())
        }
    } else if trimmed.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let action = val
                .get("action")
                .or_else(|| val.get("command"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match action.to_lowercase().as_str() {
                "play" | "resume" => IncomingPlayerCommand::Play,
                "pause" => IncomingPlayerCommand::Pause,
                "stop" => IncomingPlayerCommand::Stop,
                "seek" => {
                    let pos_ms = val.get("position_ms").and_then(|v| v.as_i64()).or_else(|| {
                        val.get("position").and_then(|v| v.as_f64()).map(|s| (s * 1000.0) as i64)
                    });
                    if let Some(pos) = pos_ms {
                        IncomingPlayerCommand::Seek(pos)
                    } else {
                        IncomingPlayerCommand::Unknown(trimmed.to_owned())
                    }
                }
                "volume" | "set_volume" => {
                    let vol = val
                        .get("volume")
                        .and_then(|v| v.as_f64())
                        .or_else(|| val.get("level").and_then(|v| v.as_f64()));
                    if let Some(v) = vol {
                        let vol_int = if (0.0..=1.0).contains(&v)
                            && !val.get("volume").is_some_and(|x| x.is_i64() && v == 1.0)
                        {
                            (v * 100.0).round() as u32
                        } else {
                            v.clamp(0.0, 100.0).round() as u32
                        };
                        IncomingPlayerCommand::SetVolume(vol_int)
                    } else {
                        IncomingPlayerCommand::Unknown(trimmed.to_owned())
                    }
                }
                _ => IncomingPlayerCommand::Unknown(trimmed.to_owned()),
            }
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
        assert_eq!(parse_incoming_command("VOLUME 0.8"), IncomingPlayerCommand::SetVolume(80));
        assert_eq!(
            parse_incoming_command("{\"action\": \"seek\", \"position\": 12.5}"),
            IncomingPlayerCommand::Seek(12500)
        );
        assert_eq!(
            parse_incoming_command("{\"action\": \"volume\", \"volume\": 50}"),
            IncomingPlayerCommand::SetVolume(50)
        );
    }

    #[test]
    fn broker_url_parsing() {
        assert_eq!(
            parse_broker_url("mqtt://192.168.68.147:1883").unwrap(),
            ("192.168.68.147".into(), 1883, false)
        );
        assert_eq!(
            parse_broker_url("mqtts://broker.example.com:8883/").unwrap(),
            ("broker.example.com".into(), 8883, true)
        );
        assert_eq!(parse_broker_url("localhost").unwrap(), ("localhost".into(), 1883, false));
    }
}
