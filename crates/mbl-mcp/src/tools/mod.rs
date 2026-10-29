//! MCP Tool definitions and execution dispatch.

pub mod curation;
pub mod hw;
pub mod import;
pub mod jobs;
pub mod libraries;
pub mod livetv;
pub mod media;
pub mod playback;
pub mod players;
pub mod remote;
pub mod settings;
pub mod system;
pub mod users;
pub mod watch;
pub mod workers;

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
    tools.extend(users::definitions());
    tools.extend(remote::definitions());
    tools.extend(settings::definitions());
    tools.extend(curation::definitions());
    tools.extend(players::definitions());
    tools.extend(workers::definitions());
    tools.extend(livetv::definitions());
    tools.extend(import::definitions());
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
        "get_hw_report"
        | "get_transcode_capacity"
        | "run_hw_selftest"
        | "create_optimized_versions"
        | "search_scenes"
        | "get_spoiler_free_recap" => hw::execute(tool_name, args, caller, services).await,
        "list_users"
        | "create_user"
        | "update_user_permissions"
        | "create_invite"
        | "revoke_sessions"
        | "delete_user" => users::execute(tool_name, args, caller, db, services).await,
        "get_remote_access_status"
        | "run_reachability_test"
        | "get_bandwidth_usage"
        | "update_remote_policy" => remote::execute(tool_name, args, caller, services).await,
        "get_settings" | "update_settings" | "create_backup" | "list_backups"
        | "restart_server" => settings::execute(tool_name, args, caller, db, services).await,
        "list_collections"
        | "create_collection"
        | "add_to_collection"
        | "create_smart_collection"
        | "list_playlists"
        | "create_playlist"
        | "add_to_playlist"
        | "list_webhooks"
        | "create_webhook"
        | "create_media_request"
        | "list_media_requests" => curation::execute(tool_name, args, caller, services).await,
        "list_players"
        | "play_on_player"
        | "control_player"
        | "set_player_tracks"
        | "stop_session"
        | "join_watch_session"
        | "send_companion_reaction"
        | "sync_playback"
        | "explain_scene" => players::execute(tool_name, args, caller, services).await,
        "list_workers" | "wake_worker" | "pair_worker" | "revoke_worker" | "list_remote_nodes" => {
            workers::execute(tool_name, args, caller, services).await
        }
        "search_guide" | "list_recordings" | "schedule_recording" | "create_series_rule"
        | "cancel_recording" | "recover_recording" => {
            livetv::execute(tool_name, args, caller, services).await
        }
        "import_plex_history" | "import_jellyfin_history" => {
            import::execute(tool_name, args, caller, services).await
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown tool: {tool_name}"))),
    }
}
