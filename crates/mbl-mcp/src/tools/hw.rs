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
        _ => Err(JsonRpcError::invalid_params(format!("unknown hw tool: {tool_name}"))),
    }
}
