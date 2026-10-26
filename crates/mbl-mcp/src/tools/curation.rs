//! MCP Curation tools: collections, playlists, webhooks, and media requests (M7.12).

use mbl_auth::tokens::Scope;
use mbl_core::{CollectionId, ItemId, PlaylistId};
use mbl_service::{
    CreateCollectionRequest, CreateMediaRequest, CreatePlaylistRequest, CreateWebhookRequest,
    Services,
};
use serde_json::{Value, json};

use crate::caller::Caller;
use crate::server::{
    CallToolResult, JsonRpcError, MutationOpts, RegisteredTool, ToolAnnotations, ToolDefinition,
    map_service_error,
};

pub fn definitions() -> Vec<RegisteredTool> {
    vec![
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "list_collections".into(),
                description: "List all curated media collections on the server.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Collections".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Library,
            definition: ToolDefinition {
                name: "create_collection".into(),
                description: "Create a new manual media collection.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Collection name" },
                        "overview": { "type": "string", "description": "Optional collection overview/description" },
                        "dry_run": { "type": "boolean", "description": "Preview without writing" }
                    },
                    "required": ["name"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Collection".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Library,
            definition: ToolDefinition {
                name: "add_to_collection".into(),
                description: "Add a media item to an existing collection.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "collection_id": { "type": "integer", "description": "Target collection ID" },
                        "item_id": { "type": "integer", "description": "Item ID to add" },
                        "dry_run": { "type": "boolean", "description": "Preview without writing" }
                    },
                    "required": ["collection_id", "item_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Add to Collection".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Library,
            definition: ToolDefinition {
                name: "create_smart_collection".into(),
                description: "Create an auto-updating smart collection based on query filters (e.g. genre, year, actor).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Smart collection name" },
                        "filter_config": { "type": "string", "description": "JSON filter rule string (e.g. {\"genre\":\"Sci-Fi\",\"minYear\":2000})" },
                        "overview": { "type": "string", "description": "Optional description" },
                        "dry_run": { "type": "boolean", "description": "Preview without writing" }
                    },
                    "required": ["name", "filter_config"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Smart Collection".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "list_playlists".into(),
                description: "List personal playlists for the current authenticated user.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Playlists".into()),
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
                name: "create_playlist".into(),
                description: "Create a new playlist for tracks, videos, or mixed media.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Playlist name" },
                        "is_public": { "type": "boolean", "description": "Whether playlist is visible to other users (default false)" }
                    },
                    "required": ["name"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Playlist".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "add_to_playlist".into(),
                description: "Add an item to a playlist.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "playlist_id": { "type": "integer", "description": "Playlist ID" },
                        "item_id": { "type": "integer", "description": "Media Item ID" }
                    },
                    "required": ["playlist_id", "item_id"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Add to Playlist".into()),
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
                name: "list_webhooks".into(),
                description: "List configured webhook notification endpoints (Admin only).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Webhooks".into()),
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
                name: "create_webhook".into(),
                description: "Register a new outgoing webhook notification endpoint for events (Admin only).".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Webhook descriptive name" },
                        "url": { "type": "string", "description": "Destination HTTP(S) URL" },
                        "events": { "type": "array", "items": { "type": "string" }, "description": "Events to listen for (e.g. [\"library.scan_finished\", \"playback.started\"])" },
                        "secret": { "type": "string", "description": "Optional HMAC-SHA256 signature secret" },
                        "dry_run": { "type": "boolean", "description": "Preview without creating" }
                    },
                    "required": ["name", "url", "events"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Webhook".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "create_media_request".into(),
                description: "Submit a user request for a missing movie or TV show via TMDB ID.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["movie", "show"], "description": "Media kind" },
                        "tmdb_id": { "type": "integer", "description": "TMDB numeric ID" },
                        "title": { "type": "string", "description": "Media title" },
                        "year": { "type": "integer", "description": "Release year" }
                    },
                    "required": ["kind", "tmdb_id", "title"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Media Request".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Read,
            definition: ToolDefinition {
                name: "list_media_requests".into(),
                description: "List submitted media requests, optionally filtered by status ('pending', 'approved', 'available').".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "description": "Optional status filter" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Media Requests".into()),
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
        "list_collections" => {
            let cols = services
                .collections
                .list_collections(&caller.actor)
                .await
                .map_err(map_service_error)?;
            Ok(CallToolResult::structured(
                &json!({ "collections": cols }),
                format!("Found {} collection(s)", cols.len()),
            ))
        }
        "create_collection" => {
            caller.require(Scope::Library).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let opts: MutationOpts = serde_json::from_value(args.clone()).unwrap_or_default();
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing required parameter: name"))?;
            let overview = args.get("overview").and_then(Value::as_str).map(String::from);

            if opts.dry_run {
                return Ok(CallToolResult::structured(
                    &json!({ "dryRun": true, "name": name, "overview": overview }),
                    format!("[DRY RUN] Would create collection '{name}'"),
                ));
            }

            let col = services
                .collections
                .create_collection(
                    &caller.actor,
                    CreateCollectionRequest {
                        name: name.to_string(),
                        overview,
                        is_smart: Some(false),
                        smart_filter: None,
                    },
                )
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "collection": col }),
                format!("Created collection '{}' (ID {})", col.name, col.id.0),
            ))
        }
        "add_to_collection" => {
            caller.require(Scope::Library).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let opts: MutationOpts = serde_json::from_value(args.clone()).unwrap_or_default();
            let col_id = args
                .get("collection_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing collection_id"))?;
            let item_id = args
                .get("item_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing item_id"))?;

            if opts.dry_run {
                return Ok(CallToolResult::structured(
                    &json!({ "dryRun": true, "collectionId": col_id, "itemId": item_id }),
                    format!("[DRY RUN] Would add item {item_id} to collection {col_id}"),
                ));
            }

            services
                .collections
                .add_item(&caller.actor, CollectionId(col_id), ItemId(item_id))
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "collectionId": col_id, "itemId": item_id }),
                format!("Added item {item_id} to collection {col_id}"),
            ))
        }
        "create_smart_collection" => {
            caller.require(Scope::Library).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let opts: MutationOpts = serde_json::from_value(args.clone()).unwrap_or_default();
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing required parameter: name"))?;
            let filter = args
                .get("filter_config")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing filter_config"))?;
            let overview = args.get("overview").and_then(Value::as_str).map(String::from);

            if opts.dry_run {
                return Ok(CallToolResult::structured(
                    &json!({ "dryRun": true, "name": name, "smartFilter": filter }),
                    format!(
                        "[DRY RUN] Would create smart collection '{name}' with filter '{filter}'"
                    ),
                ));
            }

            let col = services
                .collections
                .create_collection(
                    &caller.actor,
                    CreateCollectionRequest {
                        name: name.to_string(),
                        overview,
                        is_smart: Some(true),
                        smart_filter: Some(filter.to_string()),
                    },
                )
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "collection": col }),
                format!("Created smart collection '{}' (ID {})", col.name, col.id.0),
            ))
        }
        "list_playlists" => {
            let pls = services
                .playlists
                .list_playlists(&caller.actor)
                .await
                .map_err(map_service_error)?;
            Ok(CallToolResult::structured(
                &json!({ "playlists": pls }),
                format!("Found {} playlist(s)", pls.len()),
            ))
        }
        "create_playlist" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing required parameter: name"))?;
            let is_public = args.get("is_public").and_then(Value::as_bool);

            let pl = services
                .playlists
                .create_playlist(
                    &caller.actor,
                    CreatePlaylistRequest {
                        name: name.to_string(),
                        is_public,
                        is_smart: Some(false),
                        smart_filter: None,
                    },
                )
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "playlist": pl }),
                format!("Created playlist '{}' (ID {})", pl.name, pl.id.0),
            ))
        }
        "add_to_playlist" => {
            let pl_id = args
                .get("playlist_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing playlist_id"))?;
            let item_id = args
                .get("item_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing item_id"))?;

            services
                .playlists
                .add_item(&caller.actor, PlaylistId(pl_id), ItemId(item_id))
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "playlistId": pl_id, "itemId": item_id }),
                format!("Added item {item_id} to playlist {pl_id}"),
            ))
        }
        "list_webhooks" => {
            caller.require(Scope::Admin).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let hooks =
                services.webhooks.list_webhooks(&caller.actor).await.map_err(map_service_error)?;
            Ok(CallToolResult::structured(
                &json!({ "webhooks": hooks }),
                format!("Found {} webhook(s)", hooks.len()),
            ))
        }
        "create_webhook" => {
            caller.require(Scope::Admin).map_err(|e| JsonRpcError::forbidden(e.to_string()))?;
            let opts: MutationOpts = serde_json::from_value(args.clone()).unwrap_or_default();
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing required parameter: name"))?;
            let url = args
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing required parameter: url"))?;
            let events_vals = args.get("events").and_then(Value::as_array).ok_or_else(|| {
                JsonRpcError::invalid_params("missing required parameter: events")
            })?;
            let events: Vec<String> =
                events_vals.iter().filter_map(Value::as_str).map(String::from).collect();
            let secret = args.get("secret").and_then(Value::as_str).map(String::from);

            if opts.dry_run {
                return Ok(CallToolResult::structured(
                    &json!({ "dryRun": true, "name": name, "url": url, "events": events }),
                    format!("[DRY RUN] Would create webhook '{name}' targeting '{url}'"),
                ));
            }

            let hook = services
                .webhooks
                .create_webhook(
                    &caller.actor,
                    CreateWebhookRequest {
                        name: name.to_string(),
                        url: url.to_string(),
                        events,
                        secret,
                    },
                )
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "webhook": hook }),
                format!("Created webhook '{}' (ID {})", hook.name, hook.id.0),
            ))
        }
        "create_media_request" => {
            let kind = args
                .get("kind")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing required parameter: kind"))?;
            let tmdb_id = args
                .get("tmdb_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| JsonRpcError::invalid_params("missing tmdb_id"))?;
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .ok_or_else(|| JsonRpcError::invalid_params("missing required parameter: title"))?;
            let year = args.get("year").and_then(Value::as_i64).map(|y| y as i32);

            let req_row = services
                .requests
                .create_request(
                    &caller.actor,
                    CreateMediaRequest {
                        kind: kind.to_string(),
                        tmdb_id,
                        title: title.to_string(),
                        year,
                    },
                )
                .await
                .map_err(map_service_error)?;

            Ok(CallToolResult::structured(
                &json!({ "request": req_row }),
                format!("Created media request for '{}' (ID {})", req_row.title, req_row.id.0),
            ))
        }
        "list_media_requests" => {
            let status = args.get("status").and_then(Value::as_str);
            let reqs = services
                .requests
                .list_requests(&caller.actor, status)
                .await
                .map_err(map_service_error)?;
            Ok(CallToolResult::structured(
                &json!({ "requests": reqs }),
                format!("Found {} media request(s)", reqs.len()),
            ))
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown curation tool: {tool_name}"))),
    }
}
