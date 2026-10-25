//! MCP Watch tools: `get_continue_watching`, `get_watch_history`, `mark_watched`, `mark_unwatched`.

use mbl_auth::tokens::Scope;
use mbl_core::ItemId;
use mbl_db::Db;
use mbl_db::repo::items::{self, ItemFilter};
use mbl_service::Services;
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::sanitize::untrusted_title;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "get_continue_watching".into(),
                description: "Get items currently in progress (started but not finished) for the caller.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Maximum number of items (default 20, max 100)" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Continue Watching".into()),
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
                name: "get_watch_history".into(),
                description: "Get recent watch history events for the caller.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Maximum number of history entries (default 50, max 200)" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Watch History".into()),
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
                name: "mark_watched".into(),
                description: "Mark a media item (movie, show, season, or episode) as watched. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "item_id": { "type": "integer", "description": "Media item ID" },
                        "dry_run": { "type": "boolean", "default": false, "description": "Preview without updating watch state" },
                        "confirm": { "type": "boolean", "default": false, "description": "Confirm action" }
                    },
                    "required": ["item_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Mark Watched".into()),
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
                name: "mark_unwatched".into(),
                description: "Mark a media item (movie, show, season, or episode) as unwatched. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "item_id": { "type": "integer", "description": "Media item ID" },
                        "dry_run": { "type": "boolean", "default": false, "description": "Preview without updating watch state" },
                        "confirm": { "type": "boolean", "default": false, "description": "Confirm action" }
                    },
                    "required": ["item_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Mark Unwatched".into()),
                    read_only_hint: Some(false),
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
    db: &Db,
    services: &Services,
) -> Result<CallToolResult, JsonRpcError> {
    match tool_name {
        "get_continue_watching" => {
            let limit =
                args.get("limit").and_then(Value::as_u64).unwrap_or(20).clamp(1, 100) as u32;
            let filter = ItemFilter::for_user(&caller.actor);
            let items = items::continue_watching(db, &filter, limit)
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?;

            let sanitized: Vec<Value> = items
                .into_iter()
                .map(|item| {
                    json!({
                        "id": item.id.0,
                        "libraryId": item.library_id.0,
                        "kind": item.kind,
                        "parentId": item.parent_id.map(|p| p.0),
                        "title": untrusted_title(&item.title),
                        "year": item.year,
                        "idx": item.idx,
                        "parentIdx": item.parent_idx,
                        "rating": item.rating,
                        "durationMs": item.duration_ms,
                        "viewOffsetMs": item.view_offset_ms,
                        "viewCount": item.view_count,
                    })
                })
                .collect();

            let count = sanitized.len();
            Ok(CallToolResult::structured(
                &json!({ "items": sanitized }),
                format!("Found {count} item(s) in continue watching"),
            ))
        }
        "get_watch_history" => {
            let limit =
                args.get("limit").and_then(Value::as_u64).unwrap_or(50).clamp(1, 200) as u32;
            let history = services
                .watch
                .history(&caller.actor, limit)
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?;

            let count = history.len();
            Ok(CallToolResult::structured(
                &json!({ "history": history }),
                format!("Retrieved {count} watch history entry(ies)"),
            ))
        }
        "mark_watched" => {
            let item_id = args
                .get("item_id")
                .or_else(|| args.get("itemId"))
                .and_then(Value::as_i64)
                .map(ItemId)
                .ok_or_else(|| JsonRpcError::invalid_params("item_id is required"))?;

            let dry_run = crate::audit::extract_dry_run(args);
            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({ "dryRun": true, "itemId": item_id.0, "watched": true }),
                    format!("dry run: would mark item {} watched", item_id.0),
                ));
            }

            services.watch.mark_watched(&caller.actor, item_id).await.map_err(|e| {
                JsonRpcError::forbidden(format!("item not found or inaccessible: {e}"))
            })?;

            Ok(CallToolResult::structured(
                &json!({ "itemId": item_id.0, "watched": true }),
                format!("marked item {} watched", item_id.0),
            ))
        }
        "mark_unwatched" => {
            let item_id = args
                .get("item_id")
                .or_else(|| args.get("itemId"))
                .and_then(Value::as_i64)
                .map(ItemId)
                .ok_or_else(|| JsonRpcError::invalid_params("item_id is required"))?;

            let dry_run = crate::audit::extract_dry_run(args);
            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({ "dryRun": true, "itemId": item_id.0, "watched": false }),
                    format!("dry run: would mark item {} unwatched", item_id.0),
                ));
            }

            services.watch.mark_unwatched(&caller.actor, item_id).await.map_err(|e| {
                JsonRpcError::forbidden(format!("item not found or inaccessible: {e}"))
            })?;

            Ok(CallToolResult::structured(
                &json!({ "itemId": item_id.0, "watched": false }),
                format!("marked item {} unwatched", item_id.0),
            ))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown watch tool: {tool_name}"))),
    }
}
