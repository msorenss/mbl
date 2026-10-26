//! MCP Worker and Remote Node tools (M8.9):
//! `list_workers`, `wake_worker`, `pair_worker`, `revoke_worker`, `list_remote_nodes`.

use mbl_auth::tokens::Scope;
use mbl_core::WorkerId;
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
                name: "list_workers".into(),
                description: "List all registered distributed transcode workers, showing hostname, status, GPU capabilities, and active sessions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Transcode Workers".into()),
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
                name: "wake_worker".into(),
                description: "Transmit a Wake-on-LAN magic packet over UDP port 9 to awaken a sleeping or offline transcode worker.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "worker_id": { "type": "integer", "description": "Worker ID to wake up" }
                    },
                    "required": ["worker_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Wake Transcode Worker".into()),
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
                name: "pair_worker".into(),
                description: "Generate a one-time 6-digit pairing code valid for 15 minutes to register a new GPU transcode worker.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Generate Worker Pairing Code".into()),
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
                name: "revoke_worker".into(),
                description: "Revoke and delete a transcode worker from the cluster. Destructive: requires confirm or dry_run.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "worker_id": { "type": "integer", "description": "Worker ID to revoke" },
                        "dry_run": { "type": "boolean", "default": false, "description": "Preview revocation without deleting" },
                        "confirm": { "type": "boolean", "default": false, "description": "Confirm permanent deletion of the worker" }
                    },
                    "required": ["worker_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Revoke Transcode Worker".into()),
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
                name: "list_remote_nodes".into(),
                description: "List all federated storage nodes and their synced libraries and status.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Remote Nodes".into()),
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
        "list_workers" => {
            let workers =
                services.workers.list_workers(&caller.actor).await.map_err(map_service_error)?;

            let summary = format!("Found {} registered transcode worker(s)", workers.len());
            Ok(CallToolResult::structured(&workers, summary))
        }
        "wake_worker" => {
            let worker_id = args
                .get("worker_id")
                .and_then(Value::as_i64)
                .map(WorkerId)
                .ok_or_else(|| JsonRpcError::invalid_params("Missing worker_id"))?;

            match services.workers.wake_worker(&caller.actor, worker_id).await {
                Ok(()) => Ok(CallToolResult::structured(
                    &json!({ "workerId": worker_id.0, "woken": true }),
                    format!("Wake-on-LAN magic packet sent to worker {worker_id}"),
                )),
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "pair_worker" => {
            let code = services
                .workers
                .generate_pairing_code(&caller.actor)
                .await
                .map_err(map_service_error)?;

            let instructions = format!(
                "Run `mbl-worker pair --server <server_url> --code {}` on the worker machine.",
                code
            );

            Ok(CallToolResult::structured(
                &json!({
                    "pairingCode": code,
                    "expiresInSecs": 900,
                    "instructions": instructions
                }),
                format!("Generated worker pairing code {code} (valid for 15m)"),
            ))
        }
        "revoke_worker" => {
            let worker_id = args
                .get("worker_id")
                .and_then(Value::as_i64)
                .map(WorkerId)
                .ok_or_else(|| JsonRpcError::invalid_params("Missing worker_id"))?;

            let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
            let confirm = args.get("confirm").and_then(Value::as_bool).unwrap_or(false);
            let worker = match services.workers.get_worker(&caller.actor, worker_id).await {
                Ok(w) => w,
                Err(e) => return Ok(service_error_to_tool_result(e)),
            };

            if dry_run {
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "workerId": worker_id.0,
                        "workerName": worker.name,
                        "action": "revoke"
                    }),
                    format!("dry run: would revoke worker {} ('{}')", worker_id, worker.name),
                ));
            }

            if !confirm {
                return Ok(CallToolResult::error(format!(
                    "confirmation required: set confirm: true to proceed with revoking worker {} ('{}')",
                    worker_id, worker.name
                )));
            }

            match services.workers.delete_worker(&caller.actor, worker_id).await {
                Ok(_) => Ok(CallToolResult::structured(
                    &json!({ "workerId": worker_id.0, "revoked": true }),
                    format!("Worker {} ('{}') successfully revoked", worker_id, worker.name),
                )),
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "list_remote_nodes" => {
            let nodes =
                services.nodes.list_nodes(&caller.actor).await.map_err(map_service_error)?;

            let summary = format!("Found {} registered remote storage node(s)", nodes.len());
            Ok(CallToolResult::structured(&nodes, summary))
        }
        _ => Err(JsonRpcError::method_not_found(tool_name)),
    }
}
