//! MCP Playback tools: `list_sessions`, `explain_session`, `preview_playback`.

use mbl_auth::tokens::Scope;
use mbl_core::{ItemId, PartId};
use mbl_service::Services;
use mbl_service::playback::{PlaybackProfileInput, PlaybackRequest};
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "list_sessions".into(),
                description: "List currently active playback sessions on the server.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Playback Sessions".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "explain_session".into(),
                description: "Get detailed playback state and transcoding reasons for an active playback session.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Active playback session ID" }
                    },
                    "required": ["session_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Explain Playback Session".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "preview_playback".into(),
                description: "Simulate playback decision for a media item under a given device profile (e.g. 'chrome', 'firefox', 'safari', 'edge') without starting a session.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "item_id": { "type": "integer", "description": "Media item ID" },
                        "part_id": { "type": "integer", "description": "Optional specific media part ID" },
                        "profile": { "description": "Device profile name ('chrome', 'firefox', 'safari', 'edge') or full device profile object" },
                        "max_bitrate": { "type": "integer", "description": "Optional maximum streaming bitrate in bps" },
                        "audio_stream_index": { "type": "integer", "description": "Optional audio stream index" },
                        "subtitle_stream_index": { "type": "integer", "description": "Optional subtitle stream index" }
                    },
                    "required": ["item_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Preview Playback".into()),
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
        "list_sessions" => {
            let sessions = services.playback.list_active(&caller.actor);
            let count = sessions.len();
            Ok(CallToolResult::structured(
                &json!({ "sessions": sessions }),
                format!("{count} active playback session(s)"),
            ))
        }
        "explain_session" => {
            let session_id = args
                .get("session_id")
                .or_else(|| args.get("sessionId"))
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("session_id is required"))?;

            let session =
                services.playback.get_session(&caller.actor, session_id).ok_or_else(|| {
                    JsonRpcError::forbidden(format!(
                        "session '{session_id}' not found or inaccessible"
                    ))
                })?;

            let reasons_str = if session.reasons.is_empty() {
                "direct play".to_string()
            } else {
                session.reasons.join("; ")
            };

            let summary = format!(
                "Session {session_id}: {:?} on {} ({reasons_str})",
                session.method, session.profile_name
            );

            Ok(CallToolResult::structured(&session, summary))
        }
        "preview_playback" => {
            let item_id = args
                .get("item_id")
                .or_else(|| args.get("itemId"))
                .and_then(Value::as_i64)
                .map(ItemId)
                .ok_or_else(|| JsonRpcError::invalid_params("item_id is required"))?;

            let part_id = args
                .get("part_id")
                .or_else(|| args.get("partId"))
                .and_then(Value::as_i64)
                .map(PartId);

            let profile_input = match args.get("profile") {
                Some(Value::String(name)) => Some(PlaybackProfileInput::Named(name.clone())),
                Some(v @ Value::Object(_)) => {
                    serde_json::from_value::<PlaybackProfileInput>(v.clone()).ok()
                }
                _ => None,
            };

            let max_bitrate =
                args.get("max_bitrate").or_else(|| args.get("maxBitrate")).and_then(Value::as_u64);

            let audio_stream_index = args
                .get("audio_stream_index")
                .or_else(|| args.get("audioStreamIndex"))
                .and_then(Value::as_u64)
                .map(|u| u as u32);

            let subtitle_stream_index = args
                .get("subtitle_stream_index")
                .or_else(|| args.get("subtitleStreamIndex"))
                .and_then(Value::as_u64)
                .map(|u| u as u32);

            let req = PlaybackRequest {
                item_id,
                part_id,
                profile: profile_input,
                max_bitrate,
                audio_stream_index,
                subtitle_stream_index,
                start_ms: None,
            };

            let preview = services
                .playback
                .preview(&caller.actor, req)
                .await
                .map_err(|e| JsonRpcError::forbidden(format!("preview failed: {e}")))?;

            let reasons_str = if preview.reasons.is_empty() {
                "direct play".to_string()
            } else {
                preview.reasons.join("; ")
            };

            let summary = format!(
                "Preview for item {}: {:?} ({reasons_str})",
                preview.item_id.0, preview.method
            );

            Ok(CallToolResult::structured(&preview, summary))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown playback tool: {tool_name}"))),
    }
}
