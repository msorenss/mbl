//! MCP Migration & Import tools (M10.5):
//! `import_plex_history`, `import_jellyfin_history`.

use mbl_auth::tokens::Scope;
use mbl_core::{Actor, UserId};
use mbl_service::{
    JellyfinImportItem, JellyfinImportRequest, PlexImportItem, PlexImportRequest, Services,
};
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
            scope: Scope::Admin,
            definition: ToolDefinition {
                name: "import_plex_history".into(),
                description: "Import watch history, play counts, and resume points from Plex export format into MBL.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "userId": {
                            "type": "integer",
                            "description": "Target user ID for the imported watch states (defaults to calling admin user)"
                        },
                        "items": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "title": { "type": "string" },
                                    "year": { "type": "integer" },
                                    "imdbId": { "type": "string" },
                                    "tmdbId": { "type": "integer" },
                                    "tvdbId": { "type": "integer" },
                                    "viewCount": { "type": "integer" },
                                    "viewOffsetMs": { "type": "integer" },
                                    "lastViewedAt": { "type": "integer" }
                                },
                                "required": ["title"]
                            },
                            "description": "List of Plex metadata items to match and import"
                        }
                    },
                    "required": ["items"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Import Plex Watch History".into()),
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
                name: "import_jellyfin_history".into(),
                description: "Import watch history, play counts, and playback progress ticks from Jellyfin user data into MBL.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "userId": {
                            "type": "integer",
                            "description": "Target user ID for the imported watch states (defaults to calling admin user)"
                        },
                        "items": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "productionYear": { "type": "integer" },
                                    "played": { "type": "boolean" },
                                    "playCount": { "type": "integer" },
                                    "playbackPositionTicks": { "type": "integer" },
                                    "lastPlayedDate": { "type": "string" },
                                    "providerIds": {
                                        "type": "object",
                                        "additionalProperties": { "type": "string" }
                                    }
                                },
                                "required": ["name"]
                            },
                            "description": "List of Jellyfin UserData items to match and import"
                        }
                    },
                    "required": ["items"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Import Jellyfin Watch History".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportPlexArgs {
    user_id: Option<i64>,
    items: Vec<PlexImportItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportJellyfinArgs {
    user_id: Option<i64>,
    items: Vec<JellyfinImportItem>,
}

pub async fn execute(
    name: &str,
    args: &Value,
    caller: &Caller,
    services: &Services,
) -> Result<CallToolResult, JsonRpcError> {
    match name {
        "import_plex_history" => {
            let parsed: ImportPlexArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

            let mut actor: Actor = caller.actor.clone();
            if let Some(uid) = parsed.user_id {
                actor.user_id = UserId(uid);
            }

            let req = PlexImportRequest { items: parsed.items };

            match services.import.import_plex(&actor, req).await {
                Ok(summary) => Ok(CallToolResult::structured(
                    &summary,
                    format!(
                        "Successfully matched {} of {} items",
                        summary.matched_items, summary.total_items
                    ),
                )),
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "import_jellyfin_history" => {
            let parsed: ImportJellyfinArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(e.to_string()))?;

            let mut actor: Actor = caller.actor.clone();
            if let Some(uid) = parsed.user_id {
                actor.user_id = UserId(uid);
            }

            let req = JellyfinImportRequest { items: parsed.items };

            match services.import.import_jellyfin(&actor, req).await {
                Ok(summary) => Ok(CallToolResult::structured(
                    &summary,
                    format!(
                        "Successfully matched {} of {} items",
                        summary.matched_items, summary.total_items
                    ),
                )),
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown tool: {name}"))),
    }
}
