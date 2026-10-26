//! MCP Hardware tools (`M4.16`): `get_hw_report`, `get_transcode_capacity`, `run_hw_selftest`.

use mbl_auth::tokens::Scope;
use mbl_service::Services;
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
    service_error_to_tool_result,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "get_hw_report".into(),
                description: "Get full hardware acceleration report: platform fingerprint, NPU profile, Hailo cooperative sharing status, self-test capability matrix, transcode capacity, and recent backend failures.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Hardware Report".into()),
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
                name: "get_transcode_capacity".into(),
                description: "Get current transcode capacity, preferred backend, fallback chain, supported encode codecs, HDR tone-mapping support, ABR ladder support, and active/max video sessions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Transcode Capacity".into()),
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
                name: "run_hw_selftest".into(),
                description: "Run the functional hardware acceleration self-test across candidate backends and update the cached capability matrix and transcode capacity. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "dry_run": { "type": "boolean", "default": false, "description": "Preview candidate backends without executing FFmpeg self-tests" },
                        "confirm": { "type": "boolean", "default": false, "description": "Confirm running hardware self-test" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Run Hardware Self-Test".into()),
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
                name: "create_optimized_versions".into(),
                description: "Create an optimized H.264 High + AAC stereo MP4 version ('universal_1080p' or 'universal_720p') for an item or library so remote and incompatible clients can direct play. Supports dry_run size and duration estimate.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "item_id": { "type": "integer", "description": "Media item ID to optimize" },
                        "library_id": { "type": "integer", "description": "Optional library ID to optimize" },
                        "profile": { "type": "string", "default": "universal_1080p", "description": "Optimization profile ('universal_1080p' or 'universal_720p')" },
                        "night_window_only": { "type": "boolean", "default": false, "description": "Queue optimization for the overnight idle window" },
                        "dry_run": { "type": "boolean", "default": false, "description": "Return estimated output file size and duration without transcoding" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Optimized Versions".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "search_scenes".into(),
                description: "Semantic visual scene search across indexed movie and episode keyframes using Hailo NPU / CLIP multimodal embeddings (e.g. 'find the scene with a car chase in the snow').".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Natural-language scene description" },
                        "limit": { "type": "integer", "default": 10, "description": "Maximum number of matching scenes to return" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Semantic Scene Search".into()),
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
                name: "get_spoiler_free_recap".into(),
                description: "Generate a 'Previously On...' story recap for a TV show or season strictly from the caller's watched episodes, guaranteeing zero spoilers from unwatched episodes.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "item_id": { "type": "integer", "description": "Show or Season item ID" }
                    },
                    "required": ["item_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Spoiler-Free Recap".into()),
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
        "get_hw_report" => match services.hw.get_report(&caller.actor).await {
            Ok(report) => {
                let summary = format!(
                    "Hardware report: profile={:?}, npu={:?}, preferred={}, sessions={}/{}",
                    report.fingerprint.profile,
                    report.fingerprint.npu_profile,
                    report.capacity.preferred_backend,
                    report.capacity.active_video_sessions,
                    report.capacity.max_video_sessions
                );
                Ok(CallToolResult::structured(&report, summary))
            }
            Err(err) => Ok(service_error_to_tool_result(err)),
        },
        "get_transcode_capacity" => match services.hw.get_capacity(&caller.actor).await {
            Ok(capacity) => {
                let summary = format!(
                    "Transcode capacity: backend={}, sessions={}/{}, tonemap={}, abr_ladder={}",
                    capacity.preferred_backend,
                    capacity.active_video_sessions,
                    capacity.max_video_sessions,
                    capacity.can_tonemap,
                    capacity.supports_abr_ladder
                );
                Ok(CallToolResult::structured(&capacity, summary))
            }
            Err(err) => Ok(service_error_to_tool_result(err)),
        },
        "run_hw_selftest" => {
            let dry_run = crate::audit::extract_dry_run(args);
            if dry_run {
                let fp = services.hw.fingerprint();
                let candidates = services
                    .hw
                    .get_capacity(&caller.actor)
                    .await
                    .map(|c| c.fallback_backends)
                    .unwrap_or_else(|_| vec!["software".to_owned()]);
                return Ok(CallToolResult::structured(
                    &json!({
                        "dryRun": true,
                        "profile": fp.profile,
                        "npuProfile": fp.npu_profile,
                        "candidateBackends": candidates,
                    }),
                    format!(
                        "dry run: would test {} candidate backend(s) on {:?}",
                        candidates.len(),
                        fp.profile
                    ),
                ));
            }

            match services.hw.run_selftest(&caller.actor).await {
                Ok(res) => {
                    let ok_count = res.capabilities.iter().filter(|c| c.ok).count();
                    let summary = format!(
                        "Self-test complete: {}/{} capability probes succeeded, preferred backend={}, max sessions={}",
                        ok_count,
                        res.capabilities.len(),
                        res.capacity.preferred_backend,
                        res.capacity.max_video_sessions
                    );
                    Ok(CallToolResult::structured(&res, summary))
                }
                Err(err) => Ok(service_error_to_tool_result(err)),
            }
        }
        "create_optimized_versions" => {
            let item_id = args.get("item_id").and_then(Value::as_i64).map(mbl_core::ItemId);
            let library_id =
                args.get("library_id").and_then(Value::as_i64).map(mbl_core::LibraryId);
            let profile =
                args.get("profile").and_then(Value::as_str).unwrap_or("universal_1080p").to_owned();
            let night_window_only =
                args.get("night_window_only").and_then(Value::as_bool).unwrap_or(false);
            let dry_run = crate::audit::extract_dry_run(args);

            match services
                .intelligence
                .create_optimized(
                    &caller.actor,
                    mbl_service::CreateOptimizedRequest {
                        item_id,
                        library_id,
                        profile,
                        night_window_only,
                        dry_run,
                    },
                )
                .await
            {
                Ok(res) => {
                    let summary = if res.dry_run {
                        format!(
                            "dry run: {:?} estimated at {} bytes (~{}s)",
                            res.estimate.profile,
                            res.estimate.estimated_size_bytes,
                            res.estimate.estimated_duration_secs
                        )
                    } else {
                        format!(
                            "Optimized version created: version_id={:?}, part_id={:?}",
                            res.version_id, res.part_id
                        )
                    };
                    Ok(CallToolResult::structured(&res, summary))
                }
                Err(err) => Ok(service_error_to_tool_result(err)),
            }
        }
        "search_scenes" => {
            let query = args.get("query").and_then(Value::as_str).ok_or_else(|| {
                JsonRpcError::invalid_params("missing required string parameter 'query'")
            })?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
            match services.intelligence.search_scenes(&caller.actor, query, limit).await {
                Ok(hits) => {
                    let count = hits.len();
                    Ok(CallToolResult::structured(
                        &json!({ "hits": hits, "count": count }),
                        format!("{count} matching scene(s) for '{query}'"),
                    ))
                }
                Err(err) => Ok(service_error_to_tool_result(err)),
            }
        }
        "get_spoiler_free_recap" => {
            let item_id = args.get("item_id").and_then(Value::as_i64).ok_or_else(|| {
                JsonRpcError::invalid_params("missing required integer parameter 'item_id'")
            })?;
            match services
                .intelligence
                .spoiler_free_recap(&caller.actor, mbl_core::ItemId(item_id))
                .await
            {
                Ok(recap) => {
                    let summary = format!(
                        "Spoiler-free recap for '{}' ({} watched, {} unwatched excluded)",
                        recap.show_title,
                        recap.episodes_included,
                        recap.spoiler_protected_unwatched_excluded
                    );
                    Ok(CallToolResult::structured(&recap, summary))
                }
                Err(err) => Ok(service_error_to_tool_result(err)),
            }
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown hw tool: {tool_name}"))),
    }
}
