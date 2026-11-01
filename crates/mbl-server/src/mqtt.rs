//! MQTT Background Worker for Home Assistant discovery, live player telemetry and command dispatch.
//!
//! Synchronizes with the database settings every few seconds so that enabling/disabling MQTT or
//! modifying broker credentials takes effect without server restarts.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use mbl_core::{Actor, UserId};
use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use mbl_remote::mqtt::{
    IncomingPlayerCommand, MqttConfig, build_measurement_sensor_discovery, build_player_discovery,
    build_sensor_discovery, create_mqtt_client, parse_incoming_command,
};
use mbl_service::PlaybackState;
use mbl_service::playback::PlaybackStopRequest;
use mbl_service::players::PlayerCommand;
use rumqttc::{Event, Packet, QoS};
use serde_json::json;
use tokio_util::sync::CancellationToken;

const POLL_SETTINGS_INTERVAL: Duration = Duration::from_secs(5);
const TELEMETRY_INTERVAL: Duration = Duration::from_secs(2);

async fn desired_config(repo: &SettingsRepo) -> Option<MqttConfig> {
    let enabled = repo.get("mqtt_enabled").await.ok().flatten().as_deref() == Some("true");
    if !enabled {
        return None;
    }
    let broker_url = repo
        .get("mqtt_broker_url")
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "mqtt://localhost:1883".into());
    let username = repo.get("mqtt_username").await.ok().flatten();
    let password = repo.get("mqtt_password").await.ok().flatten();
    let topic_prefix =
        repo.get("mqtt_topic_prefix").await.ok().flatten().unwrap_or_else(|| "mbl".into());

    Some(MqttConfig { enabled: true, broker_url, username, password, topic_prefix })
}

/// Run until `stop` is cancelled, keeping the MQTT connection in sync with the database settings.
pub async fn run(db: Db, services: mbl_service::Services, stop: CancellationToken) {
    let repo = SettingsRepo::new(db);
    let mut current_config: Option<MqttConfig> = None;
    let mut worker_stop: Option<CancellationToken> = None;

    loop {
        let want = desired_config(&repo).await;
        let config_changed = match (&current_config, &want) {
            (None, None) => false,
            (Some(a), Some(b)) => {
                a.broker_url != b.broker_url
                    || a.username != b.username
                    || a.password != b.password
                    || a.topic_prefix != b.topic_prefix
            }
            _ => true,
        };

        if config_changed {
            if let Some(child_stop) = worker_stop.take() {
                child_stop.cancel();
            }
            current_config = want.clone();
            if let Some(config) = want {
                let child_stop = CancellationToken::new();
                worker_stop = Some(child_stop.clone());
                let services_clone = services.clone();
                tokio::spawn(async move {
                    run_client(config, services_clone, child_stop).await;
                });
            }
        }

        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(POLL_SETTINGS_INTERVAL) => {}
        }
    }

    if let Some(child_stop) = worker_stop {
        child_stop.cancel();
    }
}

async fn run_client(config: MqttConfig, services: mbl_service::Services, stop: CancellationToken) {
    tracing::info!(broker = %config.broker_url, prefix = %config.topic_prefix, "Starting MQTT client bridge");

    let (client, mut eventloop) = match create_mqtt_client(&config, "bridge") {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(error = %err, "Failed to create MQTT client");
            return;
        }
    };

    let server_version = env!("CARGO_PKG_VERSION");
    let status_topic = format!("{}/status", config.topic_prefix);
    let prefix = config.topic_prefix.clone();

    let mut discovered_players: HashSet<String> = HashSet::new();
    let mut last_states: HashMap<String, String> = HashMap::new();
    let mut telemetry_ticker = tokio::time::interval(TELEMETRY_INTERVAL);
    telemetry_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let admin_actor = Actor::admin(UserId(1));

    loop {
        tokio::select! {
            () = stop.cancelled() => {
                tracing::info!("Stopping MQTT client bridge");
                let _ = client.publish(&status_topic, QoS::AtLeastOnce, true, "offline").await;
                let sensor_status_topic = format!("{prefix}/sensor/server_status/state");
                let _ = client.publish(&sensor_status_topic, QoS::AtLeastOnce, true, "offline").await;
                let _ = client.disconnect().await;
                break;
            }
            _ = telemetry_ticker.tick() => {
                // 1. Broadcast server status
                let _ = client.publish(&status_topic, QoS::AtLeastOnce, true, "online").await;
                let sensor_status_topic = format!("{prefix}/sensor/server_status/state");
                let _ = client
                    .publish(&sensor_status_topic, QoS::AtLeastOnce, true, "online")
                    .await;

                // 2. Active playback sessions count sensor
                let active_sessions = services.playback.list_active(&admin_actor);
                let active_sessions_topic = format!("{prefix}/sensor/active_sessions/state");
                let _ = client.publish(
                    &active_sessions_topic,
                    QoS::AtLeastOnce,
                    false,
                    active_sessions.len().to_string(),
                ).await;

                // 3. Mirror active playback sessions into PlayerRegistry if not already present
                for session in &active_sessions {
                    let player_id = format!("session_{}", session.session_id);
                    services.players.register_or_update(mbl_service::PlayerInfo {
                        id: player_id,
                        name: format!("Playback ({})", session.profile_name),
                        device_type: session.profile_name.clone(),
                        state: match session.state {
                            PlaybackState::Playing => "playing".into(),
                            PlaybackState::Paused => "paused".into(),
                            _ => "idle".into(),
                        },
                        now_playing_item_id: Some(session.item_id),
                        position_ms: session.position_ms as i64,
                        duration_ms: session.duration_ms as i64,
                        volume: 100,
                        last_seen_at: mbl_core::time::now_ms(),
                    });
                }

                // 4. Sync registered players with Home Assistant
                if let Ok(players) = services.players.list_players(&admin_actor) {
                    let mut current_player_ids = HashSet::new();

                    for player in &players {
                        current_player_ids.insert(player.id.clone());

                        // Send discovery payload if not yet discovered
                        if !discovered_players.contains(&player.id) {
                            let (disc_topic, disc_payload) = build_player_discovery(
                                &prefix,
                                &player.id,
                                &player.name,
                                server_version,
                            );
                            if client
                                .publish(&disc_topic, QoS::AtLeastOnce, true, disc_payload)
                                .await
                                .is_ok()
                            {
                                discovered_players.insert(player.id.clone());
                                tracing::info!(player_id = %player.id, player_name = %player.name, "Published Home Assistant MQTT discovery for media player");
                            }
                        }

                        let state_str = match player.state.to_lowercase().as_str() {
                            "playing" => "playing",
                            "paused" => "paused",
                            _ => "idle",
                        };

                        let state_topic = format!("{prefix}/player/{}/state", player.id);
                        let _ = client.publish(&state_topic, QoS::AtLeastOnce, false, state_str).await;

                        let attrs_topic = format!("{prefix}/player/{}/attributes", player.id);
                        let attributes = json!({
                            "state": state_str,
                            "device_type": player.device_type,
                            "now_playing_item_id": player.now_playing_item_id.map(|id| id.0),
                            "position_ms": player.position_ms,
                            "duration_ms": player.duration_ms,
                            "volume_level": (player.volume as f64) / 100.0,
                        });
                        let _ = client.publish(
                            &attrs_topic,
                            QoS::AtLeastOnce,
                            false,
                            attributes.to_string(),
                        ).await;

                        last_states.insert(player.id.clone(), state_str.to_string());
                    }

                    // For players that disappeared, set state to "off"
                    let gone: Vec<String> = discovered_players
                        .iter()
                        .filter(|id| !current_player_ids.contains(*id))
                        .cloned()
                        .collect();

                    for player_id in gone {
                        if last_states.get(&player_id).map(|s| s.as_str()) != Some("off") {
                            let state_topic = format!("{prefix}/player/{player_id}/state");
                            let _ = client.publish(&state_topic, QoS::AtLeastOnce, false, "off").await;
                            last_states.insert(player_id.clone(), "off".to_string());
                        }
                    }
                }
            }
            event_result = eventloop.poll() => {
                match event_result {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        tracing::info!(broker = %config.broker_url, "Connected to MQTT broker");

                        // Publish initial discovery for server sensors
                        let (status_disc_topic, status_disc_payload) = build_sensor_discovery(
                            &prefix,
                            "server_status",
                            "Server Status",
                            "mdi:server",
                            server_version,
                        );
                        let _ = client.publish(&status_disc_topic, QoS::AtLeastOnce, true, status_disc_payload).await;

                        let (sessions_disc_topic, sessions_disc_payload) = build_measurement_sensor_discovery(
                            &prefix,
                            "active_sessions",
                            "Active Playback Sessions",
                            "mdi:play-circle-outline",
                            "sessions",
                            server_version,
                        );
                        let _ = client.publish(&sessions_disc_topic, QoS::AtLeastOnce, true, sessions_disc_payload).await;

                        // Subscribe to player commands
                        let player_cmd_topic = format!("{prefix}/player/+/command");
                        if let Err(e) = client.subscribe(&player_cmd_topic, QoS::AtLeastOnce).await {
                            tracing::warn!(error = %e, topic = %player_cmd_topic, "Failed to subscribe to MQTT player command topic");
                        } else {
                            tracing::info!(topic = %player_cmd_topic, "Subscribed to MQTT player command topic");
                        }
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        let topic = publish.topic.clone();
                        let payload_str = match String::from_utf8(publish.payload.to_vec()) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };

                        let expected_prefix = format!("{prefix}/player/");
                        if let Some(suffix) = topic.strip_prefix(&expected_prefix)
                            && let Some(player_id) = suffix.strip_suffix("/command")
                        {
                            let cmd = parse_incoming_command(&payload_str);
                            tracing::info!(player_id, ?cmd, "Received incoming MQTT player command");
                            dispatch_command(&services, &admin_actor, player_id, cmd).await;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "MQTT connection error; reconnecting...");
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn dispatch_command(
    services: &mbl_service::Services,
    actor: &Actor,
    player_id: &str,
    cmd: IncomingPlayerCommand,
) {
    let result = match &cmd {
        IncomingPlayerCommand::Play => {
            services.players.send_command(actor, player_id, PlayerCommand::Resume)
        }
        IncomingPlayerCommand::Pause => {
            services.players.send_command(actor, player_id, PlayerCommand::Pause)
        }
        IncomingPlayerCommand::Stop => {
            services.players.send_command(actor, player_id, PlayerCommand::Stop { message: None })
        }
        IncomingPlayerCommand::Seek(pos_ms) => services.players.send_command(
            actor,
            player_id,
            PlayerCommand::Seek { position_ms: *pos_ms },
        ),
        IncomingPlayerCommand::SetVolume(vol) => services.players.send_command(
            actor,
            player_id,
            PlayerCommand::SetVolume { volume: *vol },
        ),
        IncomingPlayerCommand::Unknown(raw) => {
            tracing::warn!(player_id, raw, "Unknown MQTT command ignored");
            return;
        }
    };

    if let Err(e) = result {
        tracing::debug!(player_id, error = %e, "Direct player WebSocket command failed, checking playback sessions");
        let session_id = player_id.strip_prefix("session_").unwrap_or(player_id);
        if let Some(session) = services.playback.get_session(actor, session_id) {
            let owner_actor = Actor::admin(session.user_id);
            if let IncomingPlayerCommand::Stop = cmd {
                let _ = services
                    .playback
                    .stop(&owner_actor, session_id, PlaybackStopRequest { position_ms: None })
                    .await;
                tracing::info!(session_id, "Stopped playback session via MQTT");
            }
        }
    }
}
