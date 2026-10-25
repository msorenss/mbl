//! MCP Tool definitions and execution dispatch.

pub mod hw;
pub mod jobs;
pub mod libraries;
pub mod media;
pub mod playback;
pub mod system;
pub mod watch;

use camino::Utf8PathBuf;
use mbl_db::Db;
use mbl_jobs::JobQueue;
use mbl_service::Services;
use serde_json::Value;

use crate::caller::Caller;
use crate::server::{CallToolResult, JsonRpcError, RegisteredTool};

/// Collect all registered tools and their scopes.
pub fn all_tools() -> Vec<RegisteredTool> {
    let mut tools = Vec::new();
    tools.extend(system::definitions());
    tools.extend(libraries::definitions());
    tools.extend(media::definitions());
    tools.extend(watch::definitions());
    tools.extend(playback::definitions());
    tools.extend(jobs::definitions());
    tools.extend(hw::definitions());
    tools
}

/// Execute a registered tool by name with provided arguments and context.
pub async fn execute_tool(
    tool_name: &str,
    args: &Value,
    caller: &Caller,
    db: &Db,
    services: &Services,
    logs_dir: Option<&Utf8PathBuf>,
    jobs: Option<&JobQueue>,
) -> Result<CallToolResult, JsonRpcError> {
    match tool_name {
        "get_server_info" | "get_health" | "get_logs" => {
            system::execute(tool_name, args, caller, services, logs_dir).await
        }
        "list_libraries" | "get_scan_status" | "scan_library" | "refresh_metadata" => {
            libraries::execute(tool_name, args, caller, db, services, jobs).await
        }
        "search_media" | "browse_items" | "get_item" | "get_children" | "get_hubs" => {
            media::execute(tool_name, args, caller, services).await
        }
        "get_continue_watching" | "get_watch_history" | "mark_watched" | "mark_unwatched" => {
            watch::execute(tool_name, args, caller, db, services).await
        }
        "list_sessions" | "explain_session" | "preview_playback" => {
            playback::execute(tool_name, args, caller, services).await
        }
        "list_jobs" | "get_job" => jobs::execute(tool_name, args, caller, db).await,
        "get_hw_report" | "get_transcode_capacity" | "run_hw_selftest" => {
            hw::execute(tool_name, args, caller, services).await
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown tool: {tool_name}"))),
    }
}
