//! MCP audit logging (`mcp_audit`), sensitive-argument redaction, and 90-day retention pruning.

use mbl_core::Result;
use mbl_core::time::now_ms;
use mbl_db::Db;
use mbl_db::repo::mcp::{self, McpAuditEntry, NewMcpAudit};
use mbl_jobs::{JobCtx, JobHandler};
use mbl_platform::PlatformProfile;
use serde_json::{Map, Value};

use crate::caller::Caller;

pub const AUDIT_RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;
pub const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

/// Check if an argument key name matches `*token* | *key* | *password* | *secret*` (case-insensitive).
pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("key")
        || lower.contains("password")
        || lower.contains("secret")
}

/// Recursively redact any JSON object fields whose keys match [`is_sensitive_key`].
pub fn redact_args(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (k, v) in map {
                if is_sensitive_key(k) {
                    out.insert(k.clone(), Value::String(REDACTED_PLACEHOLDER.into()));
                } else {
                    out.insert(k.clone(), redact_args(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_args).collect()),
        other => other.clone(),
    }
}

/// Extract whether `dry_run` was requested in tool arguments (either top-level or inside `opts`).
pub fn extract_dry_run(args: &Value) -> bool {
    args.get("dry_run")
        .or_else(|| args.get("dryRun"))
        .or_else(|| args.get("opts").and_then(|o| o.get("dry_run").or_else(|| o.get("dryRun"))))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Record a `tools/call` invocation in `mcp_audit` with redacted arguments.
pub async fn record_tool_call(
    db: &Db,
    caller: &Caller,
    tool: &str,
    args: &Value,
    outcome: &str,
    error: Option<&str>,
    duration_ms: i64,
) -> Result<i64> {
    let redacted = redact_args(args);
    let args_json = serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".into());
    let dry_run = extract_dry_run(args);

    mcp::insert_audit(
        db,
        &NewMcpAudit {
            ts: now_ms(),
            user_id: Some(caller.actor.user_id),
            token_id: caller.token_id,
            client: Some(&caller.client),
            tool,
            args: &args_json,
            outcome,
            error,
            duration_ms: duration_ms.max(0),
            dry_run,
        },
    )
    .await
}

/// List recent audit rows (`mcp_audit`).
pub async fn list_recent_audit(db: &Db, limit: u32) -> Result<Vec<McpAuditEntry>> {
    mcp::list_audit(db, limit).await
}

/// Prune `mcp_audit` rows older than 90 days relative to `now`.
pub async fn prune_expired_audit(db: &Db, now: i64) -> Result<u64> {
    let cutoff = now.saturating_sub(AUDIT_RETENTION_MS);
    mcp::prune_audit_before(db, cutoff).await
}

/// Background job handler (`mcp.audit_prune`) that deletes `mcp_audit` rows older than 90 days.
pub struct McpAuditPruneHandler {
    pub db: Db,
}

#[async_trait::async_trait]
impl JobHandler for McpAuditPruneHandler {
    fn kind(&self) -> &'static str {
        "mcp.audit_prune"
    }

    fn concurrency(&self, _profile: PlatformProfile) -> usize {
        1
    }

    async fn run(&self, _ctx: JobCtx, _payload: Value) -> Result<()> {
        let deleted = prune_expired_audit(&self.db, now_ms()).await?;
        tracing::debug!(deleted, "pruned expired mcp_audit rows");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn redacts_sensitive_keys_recursively() {
        let raw = json!({
            "query": "matrix",
            "api_key": "secret-123",
            "userPassword": "pw",
            "nested": {
                "authToken": "tok-xyz",
                "client_secret": "shh",
                "safe_field": 42
            },
            "list": [
                { "my_token": "abc", "title": "keep" }
            ]
        });
        let redacted = redact_args(&raw);
        assert_eq!(redacted["query"], "matrix");
        assert_eq!(redacted["api_key"], "[REDACTED]");
        assert_eq!(redacted["userPassword"], "[REDACTED]");
        assert_eq!(redacted["nested"]["authToken"], "[REDACTED]");
        assert_eq!(redacted["nested"]["client_secret"], "[REDACTED]");
        assert_eq!(redacted["nested"]["safe_field"], 42);
        assert_eq!(redacted["list"][0]["my_token"], "[REDACTED]");
        assert_eq!(redacted["list"][0]["title"], "keep");
    }

    #[test]
    fn extracts_dry_run_flag() {
        assert!(extract_dry_run(&json!({ "dry_run": true })));
        assert!(extract_dry_run(&json!({ "opts": { "dry_run": true } })));
        assert!(!extract_dry_run(&json!({ "query": "abc" })));
    }
}
