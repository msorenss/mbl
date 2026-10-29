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
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "recover_recording".into(),
                description: "Recover saved media from a failed or interrupted recording as an incomplete recording. Copies a stable snapshot without fetching a new broadcast; the original owner's current permissions apply. Existing series retention may delete the recovered file.".into(),
                input_schema: json!({"type":"object","properties":{
                    "recording_id":{"type":"integer","minimum":1},
                    "dry_run":{"type":"boolean","description":"Preview saved bytes and storage without creating a snapshot or catalogue entry"},
                    "confirm":{"type":"boolean","description":"Explicitly confirm recovery as an incomplete recording with its existing series policy"}
                },"required":["recording_id"],"additionalProperties":false}),
                annotations: Some(ToolAnnotations {title:Some("Recover Saved Recording".into()),read_only_hint:Some(false),destructive_hint:Some(false),idempotent_hint:Some(true),open_world_hint:Some(false)}),
            },
        },
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
                description: "List scheduled, in-progress, completed, partial, or failed TV recordings.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["scheduled", "recording", "completed", "partial", "failed", "cancelled"],
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
                        "retry_of": { "type": "integer", "description": "Retry an owned failed/partial recording, preserving its owner and current series retention policy; other schedule fields are ignored" },
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
                        "keep_max": { "type": "integer", "minimum": 1, "maximum": 10000, "description": "Keep only this many saved recordings; older saved files are deleted" },
                        "auto_delete_watched": { "type": "boolean", "description": "Delete saved recordings after the rule owner watches them" },
                        "new_only": { "type": "boolean", "description": "Only record first-run / new broadcasts" },
                        "start_padding_secs": { "type": "integer", "description": "Extra seconds before broadcast" },
                        "end_padding_secs": { "type": "integer", "description": "Extra seconds after broadcast" },
                        "dry_run": { "type": "boolean", "description": "Simulate rule creation without creating recordings" },
                        "confirm": { "type": "boolean", "description": "Explicit confirmation for rule creation" }
                    },
                    "required": ["title"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Series Recording Rule".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "cancel_recording".into(),
                description: "Cancel a scheduled or active TV recording, or delete a completed/partial recording with its saved file and catalogue item.".into(),
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
        "recover_recording" => {
            caller.require(Scope::Admin).map_err(map_service_error)?;
            let id = RecordingId(
                args.get("recording_id")
                    .and_then(Value::as_i64)
                    .filter(|id| *id > 0)
                    .ok_or_else(|| JsonRpcError::invalid_params("recording_id must be positive"))?,
            );
            if args.get("dry_run").and_then(Value::as_bool).unwrap_or(false)
                || !args.get("confirm").and_then(Value::as_bool).unwrap_or(false)
            {
                let plan = match services.livetv.preview_recording_recovery(&caller.actor, id).await
                {
                    Ok(plan) => plan,
                    Err(e) => return Ok(service_error_to_tool_result(e)),
                };
                if args.get("dry_run").and_then(Value::as_bool).unwrap_or(false) {
                    return Ok(CallToolResult::structured(
                        &json!({"dryRun":true,"plan":plan}),
                        "Recovery preview: no media or catalogue changes",
                    ));
                }
                return Ok(CallToolResult::error(
                    "confirmation required: set confirm: true to recover saved media as an incomplete recording; existing series retention applies",
                ));
            }
            match services.livetv.recover_recording(&caller.actor, id).await {
                Ok(recording) => {
                    Ok(CallToolResult::structured(&recording, "Recovered saved recording"))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "search_guide" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing query parameter"))?;
            let limit = optional_u32(args, "limit")?.unwrap_or(50);

            let programs = services
                .livetv
                .search_guide_for(&caller.actor, query, None, None, limit)
                .await
                .map_err(map_service_error)?;

            let summary = format!("Found {} guide program(s) matching '{query}'", programs.len());
            Ok(CallToolResult::structured(&programs, summary))
        }
        "list_recordings" => {
            let status = args.get("status").and_then(Value::as_str);
            let recordings = services
                .livetv
                .list_recordings_for(&caller.actor, status)
                .await
                .map_err(map_service_error)?;

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
            let start_padding_secs = optional_u32(args, "start_padding_secs")?;
            let end_padding_secs = optional_u32(args, "end_padding_secs")?;

            let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
            let confirm = args.get("confirm").and_then(Value::as_bool).unwrap_or(false);

            let req = ScheduleRecordingRequest {
                retry_of: match args.get("retry_of") {
                    None | Some(Value::Null) => None,
                    Some(value) => {
                        Some(RecordingId(value.as_i64().filter(|id| *id > 0).ok_or_else(|| {
                            JsonRpcError::invalid_params("retry_of must be a positive recording ID")
                        })?))
                    }
                },
                channel_id,
                program_id,
                title: title.to_string(),
                start_time,
                end_time,
                start_padding_secs,
                end_padding_secs,
            };
            if dry_run || !confirm {
                let plan = match services.livetv.preview_recording(&caller.actor, req).await {
                    Ok(plan) => plan,
                    Err(e) => return Ok(service_error_to_tool_result(e)),
                };
                if dry_run {
                    let mut value = serde_json::to_value(&plan)
                        .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;
                    value["dryRun"] = json!(true);
                    return Ok(CallToolResult::structured(
                        &value,
                        format!(
                            "dry run: would schedule recording '{}' on channel {}",
                            plan.title, plan.channel_id.0
                        ),
                    ));
                }
                return Ok(CallToolResult::error(format!(
                    "confirmation required: set confirm: true to schedule recording '{}'",
                    plan.title
                )));
            }

            match services.livetv.schedule_recording(&caller.actor, req).await {
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
            let season_number = optional_u32(args, "season_number")?;
            let new_only = args.get("new_only").and_then(Value::as_bool).unwrap_or(false);
            let start_padding_secs = optional_u32(args, "start_padding_secs")?;
            let end_padding_secs = optional_u32(args, "end_padding_secs")?;
            let keep_max = optional_u32(args, "keep_max")?;
            let auto_delete_watched = args.get("auto_delete_watched").and_then(Value::as_bool);

            let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
            let confirm = args.get("confirm").and_then(Value::as_bool).unwrap_or(false);

            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "title": title,
                        "channelId": channel_id.map(|c| c.0),
                        "keepMax": keep_max,
                        "autoDeleteWatched": auto_delete_watched.unwrap_or(false),
                        "newOnly": new_only
                    }),
                    format!("dry run: would create series rule for '{title}'"),
                ));
            }

            if !confirm {
                return Ok(CallToolResult::error(format!(
                    "confirmation required (retention settings may delete saved files): set confirm: true to create series rule for '{title}'"
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

            match services.livetv.create_series_rule(&caller.actor, req).await {
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

            let recording =
                match services.livetv.get_recording_for(&caller.actor, recording_id).await {
                    Ok(r) => r,
                    Err(e) => return Ok(service_error_to_tool_result(e)),
                };

            if !recording.can_delete {
                return Ok(service_error_to_tool_result(mbl_core::Error::Forbidden));
            }
            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "recordingId": recording_id.0,
                        "title": recording.title,
                        "deletesSavedFile": matches!(recording.status, mbl_core::RecordingStatus::Completed | mbl_core::RecordingStatus::Partial | mbl_core::RecordingStatus::Failed)
                    }),
                    format!(
                        "dry run: would cancel or delete recording {} ('{}') and remove any saved file",
                        recording_id.0, recording.title
                    ),
                ));
            }

            if !confirm {
                return Ok(CallToolResult::error(format!(
                    "confirmation required: set confirm: true to cancel or delete recording {} ('{}'), including its saved file",
                    recording_id.0, recording.title
                )));
            }

            match services.livetv.cancel_recording(&caller.actor, recording_id).await {
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

fn optional_u32(args: &Value, name: &str) -> Result<Option<u32>, JsonRpcError> {
    args.get(name)
        .map(|value| {
            let n = value.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| {
                JsonRpcError::invalid_params(format!("{name} must be an unsigned 32-bit integer"))
            })?;
            if (name == "keep_max" && !(1..=10000).contains(&n))
                || (name.ends_with("padding_secs") && n > 86400)
            {
                return Err(JsonRpcError::invalid_params(format!(
                    "{name} is outside its supported range"
                )));
            }
            Ok(n)
        })
        .transpose()
}
