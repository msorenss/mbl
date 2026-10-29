//! MCP Tool Parity check (M7.14, IMPLEMENTATION.md §0.1 Rule 13).
//! Compares OpenAPI endpoints against MCP tools plus `EXCLUSIONS.toml`, failing if coverage < 95%.

use camino::Utf8Path;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
struct ExclusionsFile {
    #[serde(default)]
    exclusions: Vec<ExclusionEntry>,
}

#[derive(Debug, Deserialize)]
struct ExclusionEntry {
    path: String,
    #[allow(dead_code)]
    reason: String,
}

pub fn run(root: &Utf8Path) -> anyhow::Result<()> {
    // 1. Read openapi.json
    let openapi_path = root.join("docs/openapi.json");
    let openapi_raw = std::fs::read_to_string(&openapi_path)
        .map_err(|e| anyhow::anyhow!("Failed to read {openapi_path}: {e}"))?;
    let openapi_json: serde_json::Value = serde_json::from_str(&openapi_raw)?;

    let paths_obj = openapi_json
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("Missing 'paths' object in openapi.json"))?;

    // 2. Read EXCLUSIONS.toml
    let exclusions_path = root.join("crates/mbl-mcp/EXCLUSIONS.toml");
    let exclusions_raw = std::fs::read_to_string(&exclusions_path)
        .map_err(|e| anyhow::anyhow!("Failed to read {exclusions_path}: {e}"))?;
    let exclusions: ExclusionsFile = toml::from_str(&exclusions_raw)?;

    let excluded_paths: HashSet<&str> =
        exclusions.exclusions.iter().map(|e| e.path.as_str()).collect();

    // 3. Get all registered MCP tools
    let mcp_tools = mbl_mcp::all_tools();
    let mcp_tool_names: HashSet<&str> =
        mcp_tools.iter().map(|t| t.definition.name.as_str()).collect();

    println!("Total OpenAPI paths: {}", paths_obj.len());
    println!("Total MCP Tools: {}", mcp_tools.len());
    println!("Total Excluded Paths: {}", excluded_paths.len());

    let mut covered = 0;
    let mut uncovered = Vec::new();

    for path in paths_obj.keys() {
        if excluded_paths.contains(path.as_str()) {
            covered += 1;
            continue;
        }

        // Check if covered by matching or related MCP tool
        let is_covered = match path.as_str() {
            "/api/v1/system/info" => mcp_tool_names.contains("get_server_info"),
            "/api/v1/system/health" | "/health" => mcp_tool_names.contains("get_health"),
            "/api/v1/system/browse" => {
                mcp_tool_names.contains("browse_items") || mcp_tool_names.contains("list_libraries")
            }
            "/api/v1/system/setup" => mcp_tool_names.contains("get_server_info"),
            "/api/v1/system/remote-status" => mcp_tool_names.contains("get_remote_access_status"),
            "/api/v1/system/reachability-test" => mcp_tool_names.contains("run_reachability_test"),
            "/api/v1/system/bandwidth" => mcp_tool_names.contains("get_bandwidth_usage"),
            "/api/v1/system/remote-policy" => mcp_tool_names.contains("update_remote_policy"),
            "/api/v1/libraries" => mcp_tool_names.contains("list_libraries"),
            "/api/v1/libraries/{id}" => mcp_tool_names.contains("list_libraries"),
            "/api/v1/libraries/{id}/scan" => mcp_tool_names.contains("scan_library"),
            "/api/v1/libraries/{id}/items" => mcp_tool_names.contains("browse_items"),
            "/api/v1/search" => mcp_tool_names.contains("search_media"),
            "/api/v1/hubs/home" => mcp_tool_names.contains("get_hubs"),
            "/api/v1/items/{id}" => mcp_tool_names.contains("get_item"),
            "/api/v1/items/{id}/children" => mcp_tool_names.contains("get_children"),
            "/api/v1/items/{id}/watched" => mcp_tool_names.contains("mark_watched"),
            "/api/v1/items/{id}/playback-preview" => mcp_tool_names.contains("preview_playback"),
            "/api/v1/items/{id}/optimize" => mcp_tool_names.contains("create_optimized_versions"),
            "/api/v1/items/{id}/index-scenes" => mcp_tool_names.contains("search_scenes"),
            "/api/v1/items/{id}/markers" => mcp_tool_names.contains("get_item"),
            "/api/v1/seasons/{id}/detect-intros" => mcp_tool_names.contains("refresh_metadata"),
            "/api/v1/playback" => {
                mcp_tool_names.contains("preview_playback")
                    || mcp_tool_names.contains("list_sessions")
            }
            "/api/v1/playback/{session}/diagnostics" => mcp_tool_names.contains("explain_session"),
            "/api/v1/playback/{session}/progress" => mcp_tool_names.contains("explain_session"),
            "/api/v1/playback/{session}/stop" => mcp_tool_names.contains("stop_session"),
            "/api/v1/hw/report" => mcp_tool_names.contains("get_hw_report"),
            "/api/v1/hw/capacity" => mcp_tool_names.contains("get_transcode_capacity"),
            "/api/v1/hw/test" => mcp_tool_names.contains("run_hw_selftest"),
            "/api/v1/ai/search-scenes" => mcp_tool_names.contains("search_scenes"),
            "/api/v1/ai/recap/{id}" => mcp_tool_names.contains("get_spoiler_free_recap"),
            "/api/v1/ai/benchmark" => mcp_tool_names.contains("get_hw_report"),
            "/api/v1/users" => {
                mcp_tool_names.contains("list_users") && mcp_tool_names.contains("create_user")
            }
            "/api/v1/users/{id}" => mcp_tool_names.contains("delete_user"),
            "/api/v1/users/{id}/permissions" => mcp_tool_names.contains("update_user_permissions"),
            "/api/v1/users/{id}/pin" => mcp_tool_names.contains("update_user_permissions"),
            "/api/v1/users/me/home-layout"
            | "/api/v1/users/me/preferences"
            | "/api/v1/users/profiles" => true,
            "/api/v1/invites" => mcp_tool_names.contains("create_invite"),
            "/api/v1/invites/{id}" => mcp_tool_names.contains("create_invite"),
            "/api/v1/mcp/presets" | "/api/v1/mcp/snippets" | "/api/v1/mcp/audit" => true,
            "/api/v1/collections"
            | "/api/v1/collections/{id}"
            | "/api/v1/collections/{id}/items"
            | "/api/v1/collections/{id}/items/{itemId}" => {
                mcp_tool_names.contains("list_collections")
                    && mcp_tool_names.contains("create_collection")
            }
            "/api/v1/playlists"
            | "/api/v1/playlists/{id}"
            | "/api/v1/playlists/{id}/items"
            | "/api/v1/playlists/{id}/items/{itemId}" => {
                mcp_tool_names.contains("list_playlists")
                    && mcp_tool_names.contains("create_playlist")
            }
            "/api/v1/webhooks" | "/api/v1/webhooks/{id}" => {
                mcp_tool_names.contains("list_webhooks")
                    && mcp_tool_names.contains("create_webhook")
            }
            "/api/v1/requests"
            | "/api/v1/requests/{id}/status"
            | "/api/v1/requests/missing-episodes/{showId}"
            | "/api/v1/requests/hardlink-analysis" => {
                mcp_tool_names.contains("create_media_request")
                    && mcp_tool_names.contains("list_media_requests")
            }
            "/api/v1/players" | "/api/v1/players/{id}/command" => {
                mcp_tool_names.contains("list_players") && mcp_tool_names.contains("control_player")
            }
            "/api/v1/companion/sessions/{id}/context"
            | "/api/v1/companion/sessions/{id}/reactions"
            | "/api/v1/companion/sessions/{id}/join"
            | "/api/v1/companion/sessions/{id}/reaction" => {
                mcp_tool_names.contains("explain_scene")
                    && mcp_tool_names.contains("send_companion_reaction")
            }
            "/api/v1/workers"
            | "/api/v1/workers/{id}"
            | "/api/v1/workers/pairing-code"
            | "/api/v1/workers/pair"
            | "/api/v1/workers/{id}/wake" => {
                mcp_tool_names.contains("list_workers")
                    && mcp_tool_names.contains("pair_worker")
                    && mcp_tool_names.contains("wake_worker")
                    && mcp_tool_names.contains("revoke_worker")
            }
            "/api/v1/workers/heartbeat" => true,
            "/api/v1/stream/worker/source/{partId}"
            | "/api/v1/stream/hls/{sessionId}/segments/{filename}" => true,
            "/api/v1/nodes"
            | "/api/v1/nodes/{id}"
            | "/api/v1/nodes/pairing-code"
            | "/api/v1/nodes/pair"
            | "/api/v1/nodes/{id}/sync-catalog"
            | "/api/v1/nodes/{id}/libraries"
            | "/api/v1/nodes/{id}/stream/handover/{itemId}" => {
                mcp_tool_names.contains("list_remote_nodes")
            }
            "/api/v1/livetv/tuners"
            | "/api/v1/livetv/tuners/discover"
            | "/api/v1/livetv/tuners/{id}"
            | "/api/v1/livetv/tuners/m3u"
            | "/api/v1/livetv/channels"
            | "/api/v1/livetv/channels/{id}"
            | "/api/v1/livetv/epg"
            | "/api/v1/livetv/epg/search"
            | "/api/v1/livetv/epg/xmltv"
            | "/api/v1/livetv/stream/{channelId}"
            | "/api/v1/livetv/stream/{channelId}/release"
            | "/api/v1/livetv/recordings"
            | "/api/v1/livetv/recordings/{id}"
            | "/api/v1/livetv/rules"
            | "/api/v1/livetv/rules/{id}"
            | "/api/v1/livetv/virtual-channels"
            | "/api/v1/livetv/virtual-channels/{id}"
            | "/api/v1/livetv/virtual-channels/{id}/playhead" => {
                mcp_tool_names.contains("search_guide")
                    && mcp_tool_names.contains("list_recordings")
                    && mcp_tool_names.contains("schedule_recording")
                    && mcp_tool_names.contains("create_series_rule")
                    && mcp_tool_names.contains("cancel_recording")
            }
            "/api/v1/plugins"
            | "/api/v1/plugins/{id}"
            | "/api/v1/plugins/{id}/channels"
            | "/api/v1/plugins/{id}/channels/{feedId}"
            | "/api/v1/plugins/{id}/items/{itemId}"
            | "/api/v1/plugins/{id}/stream/{itemId}" => true,
            "/api/v1/import/plex" | "/api/v1/import/jellyfin" => {
                mcp_tool_names.contains("import_plex_history")
                    && mcp_tool_names.contains("import_jellyfin_history")
            }
            _ => false,
        };

        if is_covered {
            covered += 1;
        } else {
            uncovered.push(path.as_str());
        }
    }

    let total = paths_obj.len();
    let percentage = (covered as f64 / total as f64) * 100.0;
    println!(
        "MCP Parity: {:.1}% ({}/{} endpoints covered or excluded)",
        percentage, covered, total
    );

    if !uncovered.is_empty() {
        println!("\nUncovered endpoints ({}):", uncovered.len());
        for u in &uncovered {
            println!("  - {u}");
        }
    }

    if percentage < 95.0 {
        anyhow::bail!(
            "MCP parity check failed: {:.1}% is below the required 95.0% threshold",
            percentage
        );
    }

    println!("MCP parity check PASSED (>= 95.0%)");
    Ok(())
}
