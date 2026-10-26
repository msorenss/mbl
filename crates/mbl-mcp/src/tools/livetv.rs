//! MCP Live TV and DVR tools (M9.5):
//! `search_guide`, `list_recordings`, `schedule_recording`, `create_series_rule`, `cancel_recording`.

use mbl_auth::tokens::Scope;
use mbl_core::ids::{ChannelId, ProgramId, RecordingId};
use mbl_core::livetv::{CreateRecordingRuleRequest, ScheduleRecordingRequest};
use mbl_service::Services;
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
    map_service_error, service_error_to_tool_result,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "search_guide".into(),
                description: "Search EPG program schedules across all TV channels by title, category, or description.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search keyword for program title or description" },
                        "limit": { "type": "integer", "description": "Maximum number of search results (default 50)" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Search TV Guide (EPG)".into()),
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
                name: "list_recordings".into(),
                description: "List scheduled, in-progress, completed, or failed TV recordings.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["scheduled", "recording", "completed", "failed", "cancelled"],
                            "description": "Filter recordings by status"
                        }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List DVR Recordings".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "schedule_recording".into(),
                description: "Schedule a one-off TV broadcast recording on a given channel.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "channel_id": { "type": "integer", "description": "Channel ID" },
                        "program_id": { "type": "integer", "description": "Optional EPG program ID" },
                        "title": { "type": "string", "description": "Recording title" },
                        "start_time": { "type": "integer", "description": "Start timestamp in Unix milliseconds" },
                        "end_time": { "type": "integer", "description": "End timestamp in Unix milliseconds" },
                        "start_padding_secs": { "type": "integer", "description": "Extra seconds before scheduled start" },
                        "end_padding_secs": { "type": "integer", "description": "Extra seconds after scheduled end" },
                        "dry_run": { "type": "boolean", "description": "Simulate scheduling without writing to database" },
                        "confirm": { "type": "boolean", "description": "Explicit confirmation for scheduling" }
                    },
                    "required": ["channel_id", "title", "start_time", "end_time"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Schedule Recording".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "create_series_rule".into(),
                description: "Create an automatic series recording rule to record all upcoming matching episodes.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Program/series title to match in guide" },
                        "channel_id": { "type": "integer", "description": "Optional channel ID (leave null for all channels)" },
                        "series_only": { "type": "boolean", "description": "Only record series programs" },
                        "season_number": { "type": "integer", "description": "Optional specific season to record" },
                        "new_only": { "type": "boolean", "description": "Only record first-run / new broadcasts" },
                        "start_padding_secs": { "type": "integer", "description": "Extra seconds before broadcast" },
                        "end_padding_secs": { "type": "integer", "description": "Extra seconds after broadcast" },
                        "keep_max": { "type": "integer", "description": "Maximum number of recorded episodes to keep" },
                        "auto_delete_watched": { "type": "boolean", "description": "Automatically delete watched recordings" },
                        "dry_run": { "type": "boolean", "description": "Simulate rule creation without creating recordings" },
                        "confirm": { "type": "boolean", "description": "Explicit confirmation for rule creation" }
                    },
                    "required": ["title"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Series Recording Rule".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "cancel_recording".into(),
                description: "Cancel or remove a scheduled or completed TV recording.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "recording_id": { "type": "integer", "description": "Recording ID to cancel or delete" },
                        "dry_run": { "type": "boolean", "description": "Simulate cancellation without deleting" },
                        "confirm": { "type": "boolean", "description": "Explicit confirmation for deletion" }
                    },
                    "required": ["recording_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Cancel Recording".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
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
        "search_guide" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing query parameter"))?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as u32;

            let programs = services
                .livetv
                .search_guide(query, None, None, limit)
                .await
                .map_err(map_service_error)?;

            let summary = format!("Found {} guide program(s) matching '{query}'", programs.len());
            Ok(CallToolResult::structured(&programs, summary))
        }
        "list_recordings" => {
            let status = args.get("status").and_then(Value::as_str);
            let recordings =
                services.livetv.list_recordings(status).await.map_err(map_service_error)?;

            let summary = format!("Found {} recording(s)", recordings.len());
            Ok(CallToolResult::structured(&recordings, summary))
        }
        "schedule_recording" => {
            let channel_id = args
                .get("channel_id")
                .and_then(Value::as_i64)
                .map(ChannelId)
                .ok_or_else(|| JsonRpcError::invalid_params("missing channel_id"))?;
            let program_id = args.get("program_id").and_then(Value::as_i64).map(ProgramId);
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing title"))?;
            let start_time = args
                .get("start_time")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing start_time"))?;
            let end_time = args
                .get("end_time")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing end_time"))?;
            let start_padding_secs =
                args.get("start_padding_secs").and_then(Value::as_u64).map(|p| p as u32);
            let end_padding_secs =
                args.get("end_padding_secs").and_then(Value::as_u64).map(|p| p as u32);

            let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
            let confirm = args.get("confirm").and_then(Value::as_bool).unwrap_or(false);

            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "channelId": channel_id.0,
                        "title": title,
                        "startTime": start_time,
                        "endTime": end_time
                    }),
                    format!(
                        "dry run: would schedule recording '{title}' on channel {}",
                        channel_id.0
                    ),
                ));
            }

            if !confirm {
                return Ok(CallToolResult::error(format!(
                    "confirmation required: set confirm: true to schedule recording '{title}'"
                )));
            }

            let req = ScheduleRecordingRequest {
                channel_id,
                program_id,
                title: title.to_string(),
                start_time,
                end_time,
                start_padding_secs,
                end_padding_secs,
            };

            match services.livetv.schedule_recording(req).await {
                Ok(rec) => Ok(CallToolResult::structured(
                    &rec,
                    format!("Successfully scheduled recording '{}' (ID: {})", rec.title, rec.id.0),
                )),
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "create_series_rule" => {
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing title"))?;
            let channel_id = args.get("channel_id").and_then(Value::as_i64).map(ChannelId);
            let series_only = args.get("series_only").and_then(Value::as_bool).unwrap_or(false);
            let season_number = args.get("season_number").and_then(Value::as_u64).map(|s| s as u32);
            let new_only = args.get("new_only").and_then(Value::as_bool).unwrap_or(false);
            let start_padding_secs =
                args.get("start_padding_secs").and_then(Value::as_u64).map(|p| p as u32);
            let end_padding_secs =
                args.get("end_padding_secs").and_then(Value::as_u64).map(|p| p as u32);
            let keep_max = args.get("keep_max").and_then(Value::as_u64).map(|k| k as u32);
            let auto_delete_watched = args.get("auto_delete_watched").and_then(Value::as_bool);

            let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
            let confirm = args.get("confirm").and_then(Value::as_bool).unwrap_or(false);

            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "title": title,
                        "channelId": channel_id.map(|c| c.0),
                        "newOnly": new_only
                    }),
                    format!("dry run: would create series rule for '{title}'"),
                ));
            }

            if !confirm {
                return Ok(CallToolResult::error(format!(
                    "confirmation required: set confirm: true to create series rule for '{title}'"
                )));
            }

            let req = CreateRecordingRuleRequest {
                title: title.to_string(),
                channel_id,
                series_only,
                season_number,
                new_only,
                start_padding_secs,
                end_padding_secs,
                keep_max,
                auto_delete_watched,
            };

            match services.livetv.create_series_rule(caller.actor.user_id, req).await {
                Ok(rule) => Ok(CallToolResult::structured(
                    &rule,
                    format!(
                        "Successfully created series recording rule for '{}' (ID: {})",
                        rule.title, rule.id.0
                    ),
                )),
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "cancel_recording" => {
            let recording_id = args
                .get("recording_id")
                .and_then(Value::as_i64)
                .map(RecordingId)
                .ok_or_else(|| JsonRpcError::invalid_params("missing recording_id"))?;

            let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
            let confirm = args.get("confirm").and_then(Value::as_bool).unwrap_or(false);

            let recording = match services.livetv.get_recording(recording_id).await {
                Ok(r) => r,
                Err(e) => return Ok(service_error_to_tool_result(e)),
            };

            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "recordingId": recording_id.0,
                        "title": recording.title
                    }),
                    format!(
                        "dry run: would cancel recording {} ('{}')",
                        recording_id.0, recording.title
                    ),
                ));
            }

            if !confirm {
                return Ok(CallToolResult::error(format!(
                    "confirmation required: set confirm: true to cancel recording {} ('{}')",
                    recording_id.0, recording.title
                )));
            }

            match services.livetv.cancel_recording(recording_id).await {
                Ok(_) => Ok(CallToolResult::structured(
                    &json!({ "recordingId": recording_id.0, "cancelled": true }),
                    format!(
                        "Recording {} ('{}') successfully cancelled",
                        recording_id.0, recording.title
                    ),
                )),
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        _ => Err(JsonRpcError::method_not_found(tool_name)),
    }
}
