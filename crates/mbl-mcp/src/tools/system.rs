//! MCP System tools: `get_server_info`, `get_health`, `get_logs`.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use camino::Utf8PathBuf;
use mbl_auth::tokens::Scope;
use mbl_platform::PlatformProfile;
use mbl_service::Services;
use serde_json::{Value, json};

use crate::audit::is_sensitive_key;
use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "get_server_info".into(),
                description: "Get MBL server version, OS, architecture, and platform profile."
                    .into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Server Info".into()),
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
                name: "get_health".into(),
                description: "Get server health, runtime resource stats, and active playback session count.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Server Health".into()),
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
                name: "get_logs".into(),
                description: "Get recent server log lines with optional filtering by level, timestamp, or content. Secrets and passwords are automatically redacted.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "level": { "type": "string", "description": "Filter by log level (e.g. 'error', 'warn', 'info', 'debug')" },
                        "since": { "type": "integer", "description": "Filter log events occurring on or after this timestamp in milliseconds" },
                        "contains": { "type": "string", "description": "Case-insensitive substring filter for log message" },
                        "limit": { "type": "integer", "description": "Maximum number of lines to return (default 100, max 500)" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Logs".into()),
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
    logs_dir: Option<&Utf8PathBuf>,
) -> Result<CallToolResult, JsonRpcError> {
    match tool_name {
        "get_server_info" => {
            let profile: PlatformProfile = mbl_platform::system::detect_profile();
            let info = json!({
                "name": "MBL",
                "version": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "platformProfile": profile,
            });
            Ok(CallToolResult::structured(
                &info,
                format!(
                    "MBL {} on {}/{} ({:?})",
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    profile
                ),
            ))
        }
        "get_health" => {
            let profile: PlatformProfile = mbl_platform::system::detect_profile();
            let total_mem = mbl_platform::system::total_memory_bytes();
            let active = services.playback.list_active(&caller.actor).len();
            let health = json!({
                "status": "ok",
                "platformProfile": profile,
                "totalMemoryBytes": total_mem,
                "activePlaybackSessions": active,
            });
            Ok(CallToolResult::structured(
                &health,
                format!("Server health: ok, {active} active session(s)"),
            ))
        }
        "get_logs" => {
            let level_filter =
                args.get("level").and_then(Value::as_str).map(|s| s.to_ascii_lowercase());
            let contains_filter =
                args.get("contains").and_then(Value::as_str).map(|s| s.to_ascii_lowercase());
            let limit =
                args.get("limit").and_then(Value::as_u64).unwrap_or(100).clamp(1, 500) as usize;

            let mut lines = Vec::new();
            if let Some(dir) = logs_dir
                && dir.exists()
            {
                read_log_lines(
                    dir.as_std_path(),
                    level_filter.as_deref(),
                    contains_filter.as_deref(),
                    limit,
                    &mut lines,
                );
            }

            let summary = format!("Found {} log line(s)", lines.len());
            Ok(CallToolResult::structured(
                &json!({
                    "lines": lines,
                    "count": lines.len(),
                }),
                summary,
            ))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown system tool: {tool_name}"))),
    }
}

fn read_log_lines(
    dir: &Path,
    level_filter: Option<&str>,
    contains_filter: Option<&str>,
    limit: usize,
    out: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "log"))
        .collect();
    files.sort();
    files.reverse(); // Most recent first

    for file_path in files {
        if out.len() >= limit {
            break;
        }
        if let Ok(file) = File::open(&file_path) {
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(std::result::Result::ok) {
                if let Some(lvl) = level_filter
                    && !line.to_ascii_lowercase().contains(lvl)
                {
                    continue;
                }
                if let Some(needle) = contains_filter
                    && !line.to_ascii_lowercase().contains(needle)
                {
                    continue;
                }
                let redacted = redact_log_line(&line);
                out.push(redacted);
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
}

fn redact_log_line(line: &str) -> String {
    // If line is JSON format, try parsing and redacting values with sensitive keys
    if let Ok(mut v) = serde_json::from_str::<Value>(line) {
        redact_json_value(&mut v);
        return serde_json::to_string(&v).unwrap_or_else(|_| line.to_owned());
    }

    // Otherwise, perform basic redaction for token/secret/password substrings
    let mut redacted = line.to_owned();
    for word in ["token=", "password=", "secret=", "key="] {
        if let Some(idx) = redacted.to_ascii_lowercase().find(word) {
            let val_start = idx + word.len();
            let val_end = redacted[val_start..]
                .find(|c: char| c.is_whitespace() || c == '&' || c == ',' || c == '"')
                .map(|off| val_start + off)
                .unwrap_or(redacted.len());
            if val_end > val_start {
                redacted.replace_range(val_start..val_end, "[REDACTED]");
            }
        }
    }
    redacted
}

fn redact_json_value(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if is_sensitive_key(k) {
                    *val = Value::String("[REDACTED]".into());
                } else {
                    redact_json_value(val);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}
