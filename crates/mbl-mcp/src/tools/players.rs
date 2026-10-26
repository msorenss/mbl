//! MCP Player Control tools (M7.13) and Watch Companion tools (M7.16).

use mbl_auth::tokens::Scope;
use mbl_core::ItemId;
use mbl_service::Services;
use mbl_service::playback::PlaybackStopRequest;
use mbl_service::players::PlayerCommand;
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
    map_service_error,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Playback,
            definition: ToolDefinition {
                name: "list_players".into(),
                description: "List active players (TV clients, web sessions, Chromecast, DLNA renderers) discoverable on the network.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Players".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Playback,
            definition: ToolDefinition {
                name: "play_on_player".into(),
                description: "Start playback of a media item on a remote player device.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "player_id": { "type": "string", "description": "Target player ID" },
                        "item_id": { "type": "integer", "description": "Media item ID to play" },
                        "start_ms": { "type": "integer", "description": "Optional start offset in milliseconds" }
                    },
                    "required": ["player_id", "item_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Play on Player".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Playback,
            definition: ToolDefinition {
                name: "control_player".into(),
                description: "Control playback state on a remote player (pause, resume, stop, seek, volume).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "player_id": { "type": "string", "description": "Target player ID" },
                        "action": { "type": "string", "enum": ["pause", "resume", "stop", "seek", "volume"], "description": "Control action" },
                        "position_ms": { "type": "integer", "description": "Seek position in milliseconds (required for 'seek')" },
                        "volume": { "type": "integer", "description": "Target volume 0..100 (required for 'volume')" },
                        "message": { "type": "string", "description": "Optional display message when stopping" }
                    },
                    "required": ["player_id", "action"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Control Player".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Playback,
            definition: ToolDefinition {
                name: "set_player_tracks".into(),
                description: "Switch active audio stream or subtitle track on a remote player.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "player_id": { "type": "string", "description": "Target player ID" },
                        "audio_stream_index": { "type": "integer", "description": "Target audio stream index" },
                        "subtitle_stream_index": { "type": "integer", "description": "Target subtitle stream index (-1 to disable)" }
                    },
                    "required": ["player_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Set Player Tracks".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "stop_session".into(),
                description: "Forcibly terminate an active playback stream or player session with an optional on-screen message (Admin only).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Active playback session ID" },
                        "message": { "type": "string", "description": "Explanation message displayed to the viewer" }
                    },
                    "required": ["session_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Stop Session".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Playback,
            definition: ToolDefinition {
                name: "join_watch_session".into(),
                description: "Connect an AI co-watching companion (e.g. Hermes) to a live playback session.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Active playback session ID" },
                        "bot_name": { "type": "string", "description": "AI agent identity name (e.g. 'Hermes')" }
                    },
                    "required": ["session_id", "bot_name"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Join Watch Session".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Playback,
            definition: ToolDefinition {
                name: "send_companion_reaction".into(),
                description: "Post a live commentary reaction, trivia fact, or plot explanation from the AI companion to the viewer's on-screen overlay.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Active playback session ID" },
                        "bot_name": { "type": "string", "description": "Companion agent name" },
                        "timestamp_ms": { "type": "integer", "description": "Current playback position in milliseconds" },
                        "commentary": { "type": "string", "description": "Commentary text to display" },
                        "sentiment": { "type": "string", "enum": ["funny", "shocked", "lore", "trivia", "sad"], "description": "Emotional reaction tone" }
                    },
                    "required": ["session_id", "bot_name", "commentary"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Send Companion Reaction".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Playback,
            definition: ToolDefinition {
                name: "sync_playback".into(),
                description: "Request a temporary pause or rewind during co-watching to allow conversation or explain a plot development.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "player_id": { "type": "string", "description": "Player device ID" },
                        "action": { "type": "string", "enum": ["pause", "resume", "rewind"], "description": "Sync action" },
                        "rewind_ms": { "type": "integer", "description": "Milliseconds to rewind if action is 'rewind' (default 10000)" },
                        "reason": { "type": "string", "description": "Reason shown to the user" }
                    },
                    "required": ["player_id", "action"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Sync Playback".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "explain_scene".into(),
                description: "Get detailed contextual background on the current scene (actors on screen, chapter, spoken dialogue cues).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Active playback session ID" }
                    },
                    "required": ["session_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Explain Scene".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
    ]
}

pub async fn execute(
    tool_name: &str,
    args: &Value,
    caller: &Caller,
    services: &Services,
) -> Result<CallToolResult, JsonRpcError> {
    match tool_name {
        "list_players" => {
            let players =
                services.players.list_players(&caller.actor).map_err(map_service_error)?;
            Ok(CallToolResult::structured(
                &json!({ "players": players }),
                format!("Found {} player(s)", players.len()),
            ))
        }
        "play_on_player" => {
            caller.require(Scope::Playback).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let player_id = args
                .get("player_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing player_id"))?;
            let item_id = args
                .get("item_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing item_id"))?;
            let start_ms = args.get("start_ms").and_then(Value::as_i64);

            services
                .players
                .send_command(
                    &caller.actor,
                    player_id,
                    PlayerCommand::Play { item_id: ItemId(item_id), start_ms },
                )
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "playerId": player_id, "itemId": item_id, "startMs": start_ms }),
                format!("Triggered playback of item {item_id} on player '{player_id}'"),
            ))
        }
        "control_player" => {
            caller.require(Scope::Playback).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let player_id = args
                .get("player_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing player_id"))?;
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing action"))?;

            let cmd = match action {
                "pause" => PlayerCommand::Pause,
                "resume" => PlayerCommand::Resume,
                "stop" => {
                    let msg = args.get("message").and_then(Value::as_str).map(String::from);
                    PlayerCommand::Stop { message: msg }
                }
                "seek" => {
                    let pos = args.get("position_ms").and_then(Value::as_i64).ok_or_else(|| {
                        JsonRpcError::invalid_params("missing position_ms for seek")
                    })?;
                    PlayerCommand::Seek { position_ms: pos }
                }
                "volume" => {
                    let vol = args
                        .get("volume")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| JsonRpcError::invalid_params("missing volume for volume"))?;
                    PlayerCommand::SetVolume { volume: vol as u32 }
                }
                other => {
                    return Err(JsonRpcError::invalid_params(format!(
                        "unknown control action: {other}"
                    )));
                }
            };

            services
                .players
                .send_command(&caller.actor, player_id, cmd)
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "playerId": player_id, "action": action }),
                format!("Sent command '{action}' to player '{player_id}'"),
            ))
        }
        "set_player_tracks" => {
            caller.require(Scope::Playback).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let player_id = args
                .get("player_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing player_id"))?;
            let audio_idx = args.get("audio_stream_index").and_then(Value::as_i64);
            let sub_idx = args.get("subtitle_stream_index").and_then(Value::as_i64);

            services
                .players
                .send_command(
                    &caller.actor,
                    player_id,
                    PlayerCommand::SetTracks {
                        audio_stream_index: audio_idx,
                        subtitle_stream_index: sub_idx,
                    },
                )
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "playerId": player_id, "audioStreamIndex": audio_idx, "subtitleStreamIndex": sub_idx }),
                format!(
                    "Updated audio ({audio_idx:?}) and subtitle ({sub_idx:?}) tracks on player '{player_id}'"
                ),
            ))
        }
        "stop_session" => {
            caller.require(Scope::Admin).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let session_id = args
                .get("session_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing session_id"))?;
            let message = args.get("message").and_then(Value::as_str);

            let req = PlaybackStopRequest { position_ms: None };
            services
                .playback
                .stop(&caller.actor, session_id, req)
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "sessionId": session_id, "message": message }),
                format!("Stopped session '{session_id}'"),
            ))
        }
        "join_watch_session" => {
            caller.require(Scope::Playback).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let session_id = args
                .get("session_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing session_id"))?;
            let bot_name = args
                .get("bot_name")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing bot_name"))?;

            services
                .companion
                .join_session(&caller.actor, session_id, bot_name)
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "sessionId": session_id, "botName": bot_name }),
                format!("Agent '{bot_name}' joined watch session '{session_id}'"),
            ))
        }
        "send_companion_reaction" => {
            caller.require(Scope::Playback).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let session_id = args
                .get("session_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing session_id"))?;
            let bot_name = args
                .get("bot_name")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing bot_name"))?;
            let timestamp_ms = args.get("timestamp_ms").and_then(Value::as_i64).unwrap_or(0);
            let commentary = args
                .get("commentary")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing commentary"))?;
            let sentiment = args.get("sentiment").and_then(Value::as_str);

            let reaction = services
                .companion
                .send_reaction(
                    &caller.actor,
                    session_id,
                    bot_name,
                    timestamp_ms,
                    commentary,
                    sentiment,
                )
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "reaction": reaction }),
                format!("Reaction sent from '{bot_name}': {commentary}"),
            ))
        }
        "sync_playback" => {
            caller.require(Scope::Playback).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let player_id = args
                .get("player_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing player_id"))?;
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing action"))?;
            let rewind_ms = args.get("rewind_ms").and_then(Value::as_i64).unwrap_or(10_000);

            let cmd = match action {
                "pause" => PlayerCommand::Pause,
                "resume" => PlayerCommand::Resume,
                "rewind" => {
                    let current_pos =
                        services.players.get_player(player_id).map(|p| p.position_ms).unwrap_or(0);
                    let target_pos = (current_pos - rewind_ms).max(0);
                    PlayerCommand::Seek { position_ms: target_pos }
                }
                other => {
                    return Err(JsonRpcError::invalid_params(format!(
                        "unknown sync action: {other}"
                    )));
                }
            };

            services
                .players
                .send_command(&caller.actor, player_id, cmd)
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "playerId": player_id, "action": action }),
                format!("Sync playback action '{action}' executed on player '{player_id}'"),
            ))
        }
        "explain_scene" => {
            let session_id = args
                .get("session_id")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing session_id"))?;

            let ctx = services
                .companion
                .get_live_context(&caller.actor, session_id)
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "liveContext": ctx }),
                format!(
                    "Context for session '{session_id}': title '{}' at {}s",
                    ctx.title,
                    ctx.position_ms / 1000
                ),
            ))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown player tool: {tool_name}"))),
    }
}
