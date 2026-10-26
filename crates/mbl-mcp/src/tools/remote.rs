//! MCP Remote access tools (`M6.16`): `get_remote_access_status`, `run_reachability_test`, `get_bandwidth_usage`, `update_remote_policy`.

use mbl_auth::tokens::Scope;
use mbl_service::{Services, UpdateRemotePolicyRequest};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
    service_error_to_tool_result,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Remote,
            definition: ToolDefinition {
                name: "get_remote_access_status".into(),
                description: "Get full remote access status including Tailscale node state, MagicDNS name, tailnet IPs, active peers, WAN upload limits, and active WAN streaming sessions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Remote Access Status".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Remote,
            definition: ToolDefinition {
                name: "run_reachability_test".into(),
                description: "Run Carrier-Grade NAT (CGNAT) detection and external public port reachability test to diagnose connectivity issues and generate tailored remote access recommendations.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "publicIpOverride": { "type": "string", "description": "Optional explicit public IP to test against instead of auto-probing" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Run Reachability Test".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(true),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Remote,
            definition: ToolDefinition {
                name: "get_bandwidth_usage".into(),
                description: "Inspect remote streaming bandwidth fair-share allocation, global WAN limit, and per-stream allocated bitrates.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Bandwidth Usage".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Remote,
            definition: ToolDefinition {
                name: "update_remote_policy".into(),
                description: "Update remote access policies: enforce LAN/Tailnet-only admin access, toggle Tailscale identity authentication, or configure global WAN upload limits (Mbps). Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "adminLanOnly": { "type": "boolean", "description": "Restrict admin API endpoints to LAN/Tailnet callers only" },
                        "tailscaleAuthEnabled": { "type": "boolean", "description": "Allow one-click login from Tailscale authenticated identities" },
                        "wanUploadLimitMbps": { "type": "integer", "description": "Global WAN streaming upload cap in Mbps (0 for unlimited)" },
                        "dryRun": { "type": "boolean", "description": "If true, simulates update without saving policy changes" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Update Remote Policy".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
    ]
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReachabilityTestArgs {
    public_ip_override: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRemotePolicyArgs {
    admin_lan_only: Option<bool>,
    tailscale_auth_enabled: Option<bool>,
    wan_upload_limit_mbps: Option<u64>,
    dry_run: Option<bool>,
}

pub async fn execute(
    name: &str,
    args: &Value,
    caller: &Caller,
    services: &Services,
) -> Result<CallToolResult, JsonRpcError> {
    match name {
        "get_remote_access_status" => {
            caller.require(Scope::Remote).map_err(crate::server::map_service_error)?;
            match services.remote.get_status(&caller.actor).await {
                Ok(status) => {
                    let ts_state = status
                        .tailscale
                        .as_ref()
                        .map(|t| t.backend_state.as_str())
                        .unwrap_or("Not installed / Inactive");
                    let summary = format!(
                        "Remote Status: Tailscale is '{}', Admin LAN-only is {}, WAN cap is {} Mbps ({} active sessions)",
                        ts_state,
                        status.is_admin_lan_only,
                        status.wan_upload_limit_mbps,
                        status.active_wan_sessions
                    );
                    Ok(CallToolResult::structured(&status, summary))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "run_reachability_test" => {
            caller.require(Scope::Remote).map_err(crate::server::map_service_error)?;
            let parsed: ReachabilityTestArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            match services.remote.test_reachability(&caller.actor, parsed.public_ip_override).await
            {
                Ok(report) => {
                    let summary = format!(
                        "Reachability Diagnostic: CGNAT={}, DirectReachable={}, Recommendation={:?}: {}",
                        report.is_cgnat,
                        report.is_direct_reachable,
                        report.recommendation,
                        report.details
                    );
                    Ok(CallToolResult::structured(&report, summary))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "get_bandwidth_usage" => {
            caller.require(Scope::Remote).map_err(crate::server::map_service_error)?;
            match services.remote.get_bandwidth_usage(&caller.actor).await {
                Ok(usage) => {
                    let summary = format!(
                        "Bandwidth: Upload Cap {} kbps, Active WAN Streams: {}, Fair Share per stream: {} kbps",
                        usage.global_upload_limit_kbps,
                        usage.active_wan_sessions,
                        usage.per_stream_fair_share_kbps
                    );
                    Ok(CallToolResult::structured(&usage, summary))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "update_remote_policy" => {
            caller.require(Scope::Remote).map_err(crate::server::map_service_error)?;
            let parsed: UpdateRemotePolicyArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "update_remote_policy",
                    "dryRun": true,
                    "adminLanOnly": parsed.admin_lan_only,
                    "tailscaleAuthEnabled": parsed.tailscale_auth_enabled,
                    "wanUploadLimitMbps": parsed.wan_upload_limit_mbps,
                    "willUpdate": true
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    "DRY RUN: remote access policies would be updated",
                ));
            }

            let req = UpdateRemotePolicyRequest {
                admin_lan_only: parsed.admin_lan_only,
                tailscale_auth_enabled: parsed.tailscale_auth_enabled,
                wan_upload_limit_mbps: parsed.wan_upload_limit_mbps,
            };

            match services.remote.update_policy(&caller.actor, req).await {
                Ok(status) => {
                    let summary = format!(
                        "Updated Remote Policy: admin_lan_only={}, tailscale_auth={}, wan_upload_limit={} Mbps",
                        status.is_admin_lan_only,
                        status.is_tailscale_auth_enabled,
                        status.wan_upload_limit_mbps
                    );
                    Ok(CallToolResult::structured(&status, summary))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown remote tool: {name}"))),
    }
}
