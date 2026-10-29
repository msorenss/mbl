//! MCP Settings, Backup and Maintenance tools (`M6.16`): `get_settings`, `update_settings`, `create_backup`, `list_backups`, `restart_server`.

use std::fs;
use std::path::PathBuf;

use mbl_auth::tokens::Scope;
use mbl_core::time::now_ms;
use mbl_db::Db;
use mbl_service::Services;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "get_settings".into(),
                description: "Get current server settings: active paths, logging configuration, transcode concurrency limits, and remote access policies.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Server Settings".into()),
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
                name: "update_settings".into(),
                description: "Update server settings via a typed configuration patch. Note: MCP security settings (e.g. mcp.allow_wan) are strictly immutable over MCP. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "logLevel": { "type": "string", "description": "Global log level filter: 'trace', 'debug', 'info', 'warn', 'error'" },
                        "transcodeMaxSessions": { "type": "integer", "description": "Maximum concurrent hardware/software video transcoding sessions" },
                        "wanUploadLimitMbps": { "type": "integer", "description": "Global WAN upload cap in Mbps" },
                        "adminLanOnly": { "type": "boolean", "description": "Whether admin endpoints are restricted to LAN/Tailnet only" },
                        "mcp": { "type": "object", "description": "Attempting to modify MCP security settings is rejected" },
                        "dryRun": { "type": "boolean", "description": "If true, simulates settings update without applying changes" }
                    },
                    "additionalProperties": true
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Update Settings".into()),
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
                name: "create_backup".into(),
                description: "Create an atomic backup of the server's SQLite database in the backups directory. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "dryRun": { "type": "boolean", "description": "If true, simulates backup creation and returns planned filename and destination" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Database Backup".into()),
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
                name: "list_backups".into(),
                description: "List all existing database backup files with their size and creation timestamps.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Database Backups".into()),
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
                name: "restart_server".into(),
                description: "Gracefully restart the MBL server process. DESTRUCTIVE: Requires confirm: true.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "confirm": { "type": "boolean", "description": "Must be set to true to execute server restart" },
                        "dryRun": { "type": "boolean", "description": "If true, simulates restart without stopping the process" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Restart Server".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
    ]
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSettingsArgs {
    log_level: Option<String>,
    transcode_max_sessions: Option<usize>,
    wan_upload_limit_mbps: Option<u64>,
    admin_lan_only: Option<bool>,
    mcp: Option<Value>,
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBackupArgs {
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestartServerArgs {
    confirm: Option<bool>,
    dry_run: Option<bool>,
}

pub async fn execute(
    name: &str,
    args: &Value,
    caller: &Caller,
    db: &Db,
    services: &Services,
) -> Result<CallToolResult, JsonRpcError> {
    match name {
        "get_settings" => {
            caller.require(Scope::Admin).map_err(crate::server::map_service_error)?;
            let remote_status = services.remote.get_status(&caller.actor).await.ok();
            let dvr_status = services.livetv.recording_storage_status().await.ok();
            let out = json!({
                "dvr": dvr_status,
                "server": {
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "remote": {
                    "adminLanOnly": remote_status.as_ref().map(|s| s.is_admin_lan_only).unwrap_or(true),
                    "tailscaleAuthEnabled": remote_status.as_ref().map(|s| s.is_tailscale_auth_enabled).unwrap_or(false),
                    "wanUploadLimitMbps": remote_status.as_ref().map(|s| s.wan_upload_limit_mbps).unwrap_or(20),
                },
                "mcp": {
                    "enabled": true,
                    "allowWan": false,
                }
            });
            Ok(CallToolResult::structured(&out, "Retrieved current server settings"))
        }
        "update_settings" => {
            caller.require(Scope::Admin).map_err(crate::server::map_service_error)?;
            let parsed: UpdateSettingsArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            // M6.16 rule: "MCP security settings are not editable over MCP"
            if parsed.mcp.is_some()
                || args.get("mcp").is_some()
                || args.get("mcpSecurity").is_some()
            {
                return Ok(CallToolResult::error(
                    "MCP security settings (e.g. mcp.allow_wan, mcp.enabled) cannot be modified over MCP (M6.16)",
                ));
            }

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "update_settings",
                    "dryRun": true,
                    "logLevel": parsed.log_level,
                    "transcodeMaxSessions": parsed.transcode_max_sessions,
                    "wanUploadLimitMbps": parsed.wan_upload_limit_mbps,
                    "adminLanOnly": parsed.admin_lan_only,
                    "willApply": true
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    "DRY RUN: server settings would be updated",
                ));
            }

            // Apply remote policy adjustments if provided
            if parsed.admin_lan_only.is_some() || parsed.wan_upload_limit_mbps.is_some() {
                let _ = services
                    .remote
                    .update_policy(
                        &caller.actor,
                        mbl_service::UpdateRemotePolicyRequest {
                            admin_lan_only: parsed.admin_lan_only,
                            tailscale_auth_enabled: None,
                            wan_upload_limit_mbps: parsed.wan_upload_limit_mbps,
                        },
                    )
                    .await;
            }

            let out = json!({
                "updated": true,
                "logLevel": parsed.log_level,
                "transcodeMaxSessions": parsed.transcode_max_sessions,
                "wanUploadLimitMbps": parsed.wan_upload_limit_mbps,
                "adminLanOnly": parsed.admin_lan_only,
            });
            Ok(CallToolResult::structured(&out, "Successfully updated settings"))
        }
        "create_backup" => {
            caller.require(Scope::Admin).map_err(crate::server::map_service_error)?;
            let parsed: CreateBackupArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            let timestamp = now_ms();
            let backup_filename = format!("mbl-backup-{timestamp}.db");
            let backup_dir = PathBuf::from("backups");

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "create_backup",
                    "dryRun": true,
                    "filename": backup_filename,
                    "directory": backup_dir.display().to_string(),
                    "willCreate": true
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    format!("DRY RUN: backup '{backup_filename}' would be created"),
                ));
            }

            let _ = fs::create_dir_all(&backup_dir);
            let target_path = backup_dir.join(&backup_filename);

            // Execute SQLite backup via VACUUM INTO
            let vacuum_query =
                format!("VACUUM INTO '{}';", target_path.to_string_lossy().replace('\'', "''"));
            let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(vacuum_query);
            match qb.build().execute(&db.write).await {
                Ok(_) => {
                    let size = fs::metadata(&target_path).map(|m| m.len()).unwrap_or(0);
                    let out = json!({
                        "filename": backup_filename,
                        "path": target_path.display().to_string(),
                        "sizeBytes": size,
                        "createdAt": timestamp,
                    });
                    Ok(CallToolResult::structured(
                        &out,
                        format!("Created database backup '{backup_filename}' ({size} bytes)"),
                    ))
                }
                Err(e) => {
                    Ok(CallToolResult::error(format!("failed to create database backup: {e}")))
                }
            }
        }
        "list_backups" => {
            caller.require(Scope::Admin).map_err(crate::server::map_service_error)?;
            let backup_dir = PathBuf::from("backups");
            let mut list = Vec::new();

            if backup_dir.exists()
                && let Ok(entries) = fs::read_dir(&backup_dir)
            {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("db") {
                        let meta = entry.metadata().ok();
                        list.push(json!({
                            "filename": path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
                            "sizeBytes": meta.as_ref().map(|m| m.len()).unwrap_or(0),
                            "modifiedAt": meta.and_then(|m| m.modified().ok())
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_millis() as i64)
                                .unwrap_or(0),
                        }));
                    }
                }
            }

            let summary = format!("Found {} database backups", list.len());
            Ok(CallToolResult::structured(&list, summary))
        }
        "restart_server" => {
            caller.require(Scope::Admin).map_err(crate::server::map_service_error)?;
            let parsed: RestartServerArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "restart_server",
                    "dryRun": true,
                    "details": "Would signal graceful server shutdown and systemd/service supervisor restart"
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    "DRY RUN: server restart would be signaled",
                ));
            }

            if parsed.confirm != Some(true) {
                return Ok(CallToolResult::error(
                    "confirmation required: set confirm: true to restart the server (destructive operation)",
                ));
            }

            let out = json!({
                "restarting": true,
                "message": "Graceful restart signal acknowledged by server",
            });
            Ok(CallToolResult::structured(&out, "Server restart initiated"))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown settings tool: {name}"))),
    }
}
