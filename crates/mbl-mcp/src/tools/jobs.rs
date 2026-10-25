//! MCP Job tools: `list_jobs`, `get_job`.

use mbl_auth::tokens::Scope;
use mbl_core::JobId;
use mbl_db::Db;
use mbl_db::repo::jobs::JobsRepo;
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, RegisteredTool, ToolAnnotations, ToolDefinition,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "list_jobs".into(),
                description: "List recent background tasks (scans, metadata refreshes, transcodes) and their statuses.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Maximum number of jobs to return (default 50, max 200)" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Jobs".into()),
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
                name: "get_job".into(),
                description: "Get status, progress, and error details for a specific background job by ID.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "job_id": { "type": "integer", "description": "Job ID" }
                    },
                    "required": ["job_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Get Job".into()),
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
    _caller: &Caller,
    db: &Db,
) -> Result<CallToolResult, JsonRpcError> {
    match tool_name {
        "list_jobs" => {
            let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(50).clamp(1, 200);
            let repo = JobsRepo::new(db.clone());
            let jobs =
                repo.list_recent(limit).await.map_err(|e| JsonRpcError::internal(e.to_string()))?;

            let count = jobs.len();
            Ok(CallToolResult::structured(
                &json!({ "jobs": jobs }),
                format!("Retrieved {count} job(s)"),
            ))
        }
        "get_job" => {
            let job_id = args
                .get("job_id")
                .or_else(|| args.get("jobId"))
                .and_then(Value::as_i64)
                .map(JobId)
                .ok_or_else(|| JsonRpcError::invalid_params("job_id is required"))?;

            let repo = JobsRepo::new(db.clone());
            let job = repo
                .get(job_id)
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?
                .ok_or_else(|| {
                    JsonRpcError::invalid_params(format!("job {} not found", job_id.0))
                })?;

            let summary = format!("Job {}: {} ({})", job.id.0, job.kind, job.status);
            Ok(CallToolResult::structured(&job, summary))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown job tool: {tool_name}"))),
    }
}
