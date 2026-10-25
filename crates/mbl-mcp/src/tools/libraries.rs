//! MCP Library tools: `list_libraries`, `get_scan_status`, `scan_library`, `refresh_metadata`.

use mbl_auth::tokens::Scope;
use mbl_core::{ItemId, LibraryId};
use mbl_db::Db;
use mbl_db::repo::jobs::JobsRepo;
use mbl_jobs::{JobQueue, Priority, kinds};
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
                name: "list_libraries".into(),
                description: "List all media libraries accessible to the caller.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Libraries".into()),
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
                name: "get_scan_status".into(),
                description: "Check whether a library scan is currently running or queued.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "library_id": { "type": "integer", "description": "Optional library ID to check" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Scan Status".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Library,
            definition: ToolDefinition {
                name: "scan_library".into(),
                description: "Start scanning a library to detect new or modified media files. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "library_id": { "type": "integer", "description": "Library ID to scan" },
                        "dry_run": { "type": "boolean", "default": false, "description": "Preview what would be scanned without enqueueing the job" },
                        "confirm": { "type": "boolean", "default": false, "description": "Confirm execution of library scan" }
                    },
                    "required": ["library_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Scan Library".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Library,
            definition: ToolDefinition {
                name: "refresh_metadata".into(),
                description: "Trigger metadata and artwork refresh for a library or a specific item. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "library_id": { "type": "integer", "description": "Optional library ID to refresh" },
                        "item_id": { "type": "integer", "description": "Optional media item ID to refresh" },
                        "dry_run": { "type": "boolean", "default": false, "description": "Preview the refresh action without enqueueing" },
                        "confirm": { "type": "boolean", "default": false, "description": "Confirm metadata refresh" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Refresh Metadata".into()),
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
    jobs: Option<&JobQueue>,
) -> Result<CallToolResult, JsonRpcError> {
    match tool_name {
        "list_libraries" => {
            let libs = services
                .libraries
                .list(&caller.actor)
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?;
            let sanitized: Vec<Value> = libs
                .into_iter()
                .map(|lib| {
                    json!({
                        "id": lib.id.0,
                        "name": untrusted_title(&lib.name),
                        "kind": lib.kind,
                        "paths": lib.paths,
                        "createdAt": lib.created_at,
                    })
                })
                .collect();
            let count = sanitized.len();
            Ok(CallToolResult::structured(
                &json!({ "libraries": sanitized }),
                format!("{count} library(ies) accessible"),
            ))
        }
        "get_scan_status" => {
            let lib_filter =
                args.get("library_id").or_else(|| args.get("libraryId")).and_then(Value::as_i64);
            let jobs_repo = JobsRepo::new(db.clone());
            let recent_jobs = jobs_repo
                .list_recent(50)
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?;
            let scan_jobs: Vec<_> = recent_jobs
                .into_iter()
                .filter(|j| j.kind == kinds::LIBRARY_SCAN)
                .filter(|j| {
                    if let Some(target_lib) = lib_filter {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&j.payload)
                            && let Some(lid) = parsed.get("libraryId").and_then(Value::as_i64)
                        {
                            return lid == target_lib;
                        }
                        return false;
                    }
                    true
                })
                .collect();

            let active_scan =
                scan_jobs.iter().find(|j| j.status == "running" || j.status == "queued");
            let is_scanning = active_scan.is_some();
            let summary = if is_scanning {
                "A library scan is currently active or queued"
            } else {
                "No active library scan"
            };

            Ok(CallToolResult::structured(
                &json!({
                    "isScanning": is_scanning,
                    "activeScanJob": active_scan,
                    "recentScans": scan_jobs.iter().take(5).collect::<Vec<_>>(),
                }),
                summary,
            ))
        }
        "scan_library" => {
            let lib_id = args
                .get("library_id")
                .or_else(|| args.get("libraryId"))
                .and_then(Value::as_i64)
                .map(LibraryId)
                .ok_or_else(|| JsonRpcError::invalid_params("library_id is required"))?;
            let dry_run = crate::audit::extract_dry_run(args);
            if dry_run {
                let lib = services.libraries.get(&caller.actor, lib_id).await.map_err(|e| {
                    JsonRpcError::forbidden(format!("library not found or inaccessible: {e}"))
                })?;
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "libraryId": lib.id.0,
                        "name": untrusted_title(&lib.name),
                        "paths": lib.paths,
                    }),
                    format!("dry run: would scan library '{}'", lib.name),
                ));
            }

            let job_id = services
                .libraries
                .scan(&caller.actor, lib_id)
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?;
            Ok(CallToolResult::structured(
                &json!({ "jobId": job_id.0, "libraryId": lib_id.0 }),
                format!("scan queued as job {}", job_id.0),
            ))
        }
        "refresh_metadata" => {
            let lib_id = args
                .get("library_id")
                .or_else(|| args.get("libraryId"))
                .and_then(Value::as_i64)
                .map(LibraryId);
            let item_id = args
                .get("item_id")
                .or_else(|| args.get("itemId"))
                .and_then(Value::as_i64)
                .map(ItemId);

            if lib_id.is_none() && item_id.is_none() {
                return Err(JsonRpcError::invalid_params(
                    "either library_id or item_id is required",
                ));
            }

            let dry_run = crate::audit::extract_dry_run(args);
            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "libraryId": lib_id.map(|l| l.0),
                        "itemId": item_id.map(|i| i.0),
                    }),
                    "dry run: would refresh metadata",
                ));
            }

            let payload = if let Some(item) = item_id {
                json!({ "itemId": item.0 })
            } else if let Some(lib) = lib_id {
                json!({ "libraryId": lib.0 })
            } else {
                return Err(JsonRpcError::invalid_params(
                    "either library_id or item_id is required",
                ));
            };

            let job_id = if let Some(queue) = jobs {
                queue
                    .enqueue(kinds::METADATA_REFRESH, &payload, Priority::Normal)
                    .await
                    .map_err(|e| JsonRpcError::internal(e.to_string()))?
            } else {
                let repo = JobsRepo::new(db.clone());
                repo.insert(
                    kinds::METADATA_REFRESH,
                    &payload.to_string(),
                    Priority::Normal as i64,
                    mbl_core::time::now_ms(),
                )
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?
            };

            Ok(CallToolResult::structured(
                &json!({ "jobId": job_id.0 }),
                format!("metadata refresh queued as job {}", job_id.0),
            ))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown library tool: {tool_name}"))),
    }
}
