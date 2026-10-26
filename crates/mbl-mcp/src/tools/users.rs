//! MCP User management tools (`M6.16`): `list_users`, `create_user`, `update_user_permissions`, `create_invite`, `revoke_sessions`, `delete_user`.

use mbl_auth::tokens::Scope;
use mbl_core::{LibraryId, UserId};
use mbl_db::Db;
use mbl_service::{CreateInviteRequest, CreateUser, Services, UpdateUserPermissionsRequest};
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
            scope: Scope::Users,
            definition: ToolDefinition {
                name: "list_users".into(),
                description: "List all user accounts on the server with their roles, managed status, and library permissions.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("List Users".into()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Users,
            definition: ToolDefinition {
                name: "create_user".into(),
                description: "Create a new user account with specified credentials and optional parental controls/library restrictions. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "username": { "type": "string", "description": "Unique username (3-32 alphanumeric/underscore characters)" },
                        "password": { "type": "string", "description": "User password (minimum 8 characters)" },
                        "displayName": { "type": "string", "description": "Human-friendly display name" },
                        "isAdmin": { "type": "boolean", "description": "Whether this user has full administrator privileges (default: false)" },
                        "isManaged": { "type": "boolean", "description": "Whether this is a managed child profile (default: false)" },
                        "contentRatingCeiling": { "type": "string", "description": "Maximum allowed rating, e.g. 'PG-13', 'TV-14'" },
                        "libraryIds": {
                            "type": "array",
                            "items": { "type": "integer" },
                            "description": "Specific library IDs the user is allowed to access (empty/null allows all libraries)"
                        },
                        "dryRun": { "type": "boolean", "description": "If true, simulates user creation and returns a plan without modifying the database" }
                    },
                    "required": ["username", "password"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create User".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Users,
            definition: ToolDefinition {
                name: "update_user_permissions".into(),
                description: "Update library access rules, managed status, and content rating ceilings for a user. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "userId": { "type": "integer", "description": "ID of the user to update" },
                        "isManaged": { "type": "boolean", "description": "Whether the user is a managed profile" },
                        "contentRatingCeiling": { "type": "string", "description": "Content rating ceiling, e.g. 'G', 'PG', 'PG-13', 'R'" },
                        "libraryIds": {
                            "type": "array",
                            "items": { "type": "integer" },
                            "description": "Allowed library IDs (null or omitted preserves existing; empty array clears restrictions)"
                        },
                        "dryRun": { "type": "boolean", "description": "If true, previews updates without applying changes" }
                    },
                    "required": ["userId"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Update User Permissions".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Users,
            definition: ToolDefinition {
                name: "create_invite".into(),
                description: "Create a shareable invite code with pre-configured library access and content rating ceiling. Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "libraryIds": {
                            "type": "array",
                            "items": { "type": "integer" },
                            "description": "Library IDs granted to new user upon registration"
                        },
                        "contentRatingCeiling": { "type": "string", "description": "Maximum content rating" },
                        "ttlHours": { "type": "integer", "description": "Validity period in hours (default: 72)" },
                        "dryRun": { "type": "boolean", "description": "If true, simulates creation and returns plan without writing invite" }
                    },
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Create Share Invite".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Users,
            definition: ToolDefinition {
                name: "revoke_sessions".into(),
                description: "Revoke active login sessions for a user (or a specific session ID). Supports dry_run preview.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "userId": { "type": "integer", "description": "User ID whose sessions should be revoked" },
                        "sessionId": { "type": "string", "description": "Optional specific session ID to revoke. If omitted, revokes all sessions for user." },
                        "dryRun": { "type": "boolean", "description": "If true, returns count of sessions that would be revoked" }
                    },
                    "required": ["userId"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Revoke Sessions".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            },
        },
        RegisteredTool {
            scope: Scope::Users,
            definition: ToolDefinition {
                name: "delete_user".into(),
                description: "Permanently delete a user account and revoke all their sessions. DESTRUCTIVE: Requires confirm: true.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "userId": { "type": "integer", "description": "ID of user to delete" },
                        "confirm": { "type": "boolean", "description": "Must be set to true to execute this destructive operation" },
                        "dryRun": { "type": "boolean", "description": "If true, returns user deletion preview without deleting" }
                    },
                    "required": ["userId"],
                    "additionalProperties": false
                }),
                annotations: Some(ToolAnnotations {
                    title: Some("Delete User".into()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                }),
            },
        },
    ]
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserArgs {
    username: String,
    password: String,
    display_name: Option<String>,
    is_admin: Option<bool>,
    is_managed: Option<bool>,
    content_rating_ceiling: Option<String>,
    library_ids: Option<Vec<i64>>,
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePermissionsArgs {
    user_id: i64,
    is_managed: Option<bool>,
    content_rating_ceiling: Option<String>,
    library_ids: Option<Vec<i64>>,
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInviteArgs {
    library_ids: Option<Vec<i64>>,
    content_rating_ceiling: Option<String>,
    ttl_hours: Option<u32>,
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevokeSessionsArgs {
    user_id: i64,
    session_id: Option<i64>,
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteUserArgs {
    user_id: i64,
    confirm: Option<bool>,
    dry_run: Option<bool>,
}

pub async fn execute(
    name: &str,
    args: &Value,
    caller: &Caller,
    db: &Db,
    services: &Services,
) -> Result<CallToolResult, JsonRpcError> {
    match name {
        "list_users" => {
            caller.require(Scope::Users).map_err(crate::server::map_service_error)?;
            match services.users.list(&caller.actor).await {
                Ok(users) => {
                    let mut detailed = Vec::new();
                    for u in &users {
                        let sec = services.users.get_security_info(&caller.actor, u.id).await.ok();
                        detailed.push(json!({
                            "id": u.id.0,
                            "username": u.username,
                            "displayName": u.display_name,
                            "isAdmin": u.is_admin,
                            "isManaged": sec.as_ref().map(|s| s.is_managed).unwrap_or(false),
                            "contentRatingCeiling": sec.as_ref().and_then(|s| s.content_rating_ceiling.clone()),
                            "hasPin": sec.as_ref().map(|s| s.has_pin).unwrap_or(false),
                            "totpEnabled": sec.as_ref().map(|s| s.totp_enabled).unwrap_or(false),
                            "libraryIds": sec.as_ref().map(|s| s.library_ids.iter().map(|id| id.0).collect::<Vec<_>>()).unwrap_or_default(),
                        }));
                    }
                    let summary = format!("Retrieved {} users", detailed.len());
                    Ok(CallToolResult::structured(&detailed, summary))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "create_user" => {
            caller.require(Scope::Users).map_err(crate::server::map_service_error)?;
            let parsed: CreateUserArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "create_user",
                    "dryRun": true,
                    "username": parsed.username,
                    "displayName": parsed.display_name.unwrap_or_else(|| parsed.username.clone()),
                    "isAdmin": parsed.is_admin.unwrap_or(false),
                    "isManaged": parsed.is_managed.unwrap_or(false),
                    "contentRatingCeiling": parsed.content_rating_ceiling,
                    "libraryIds": parsed.library_ids,
                    "willCreate": true
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    format!("DRY RUN: user '{}' would be created", parsed.username),
                ));
            }

            let input = CreateUser {
                username: parsed.username,
                password: parsed.password,
                display_name: parsed.display_name,
                is_admin: parsed.is_admin.unwrap_or(false),
                is_managed: parsed.is_managed.unwrap_or(false),
                content_rating_ceiling: parsed.content_rating_ceiling,
                library_ids: parsed.library_ids.map(|ids| ids.into_iter().map(LibraryId).collect()),
            };

            match services.users.create(&caller.actor, input).await {
                Ok(user) => {
                    let out = json!({
                        "id": user.id.0,
                        "username": user.username,
                        "displayName": user.display_name,
                        "isAdmin": user.is_admin,
                    });
                    Ok(CallToolResult::structured(
                        &out,
                        format!("Created user '{}' (id: {})", user.username, user.id.0),
                    ))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "update_user_permissions" => {
            caller.require(Scope::Users).map_err(crate::server::map_service_error)?;
            let parsed: UpdatePermissionsArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            let uid = UserId(parsed.user_id);

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "update_user_permissions",
                    "dryRun": true,
                    "userId": parsed.user_id,
                    "isManaged": parsed.is_managed,
                    "contentRatingCeiling": parsed.content_rating_ceiling,
                    "libraryIds": parsed.library_ids,
                    "willUpdate": true
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    format!("DRY RUN: permissions for user {} would be updated", parsed.user_id),
                ));
            }

            let req = UpdateUserPermissionsRequest {
                is_managed: parsed.is_managed,
                content_rating_ceiling: parsed.content_rating_ceiling.map(Some),
                library_ids: parsed.library_ids.map(|ids| ids.into_iter().map(LibraryId).collect()),
            };

            match services.users.update_permissions(&caller.actor, uid, req).await {
                Ok(sec) => {
                    let out = json!({
                        "userId": sec.user_id.0,
                        "isManaged": sec.is_managed,
                        "contentRatingCeiling": sec.content_rating_ceiling,
                        "libraryIds": sec.library_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
                    });
                    Ok(CallToolResult::structured(
                        &out,
                        format!("Updated permissions for user {}", parsed.user_id),
                    ))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "create_invite" => {
            caller.require(Scope::Users).map_err(crate::server::map_service_error)?;
            let parsed: CreateInviteArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "create_invite",
                    "dryRun": true,
                    "libraryIds": parsed.library_ids,
                    "contentRatingCeiling": parsed.content_rating_ceiling,
                    "ttlHours": parsed.ttl_hours.unwrap_or(72),
                    "willCreate": true
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    "DRY RUN: share invite would be generated",
                ));
            }

            let req = CreateInviteRequest {
                library_ids: parsed
                    .library_ids
                    .unwrap_or_default()
                    .into_iter()
                    .map(LibraryId)
                    .collect(),
                content_rating_ceiling: parsed.content_rating_ceiling,
                ttl_hours: parsed.ttl_hours.unwrap_or(72),
            };

            match services.auth.create_invite(&caller.actor, req).await {
                Ok(inv) => {
                    let out = json!({
                        "id": inv.id,
                        "code": inv.code,
                        "expiresAt": inv.expires_at,
                        "libraryIds": inv.library_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
                        "contentRatingCeiling": inv.content_rating_ceiling,
                    });
                    Ok(CallToolResult::structured(
                        &out,
                        format!("Created invite code: {}", inv.code),
                    ))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        "revoke_sessions" => {
            caller.require(Scope::Users).map_err(crate::server::map_service_error)?;
            let parsed: RevokeSessionsArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            let uid = UserId(parsed.user_id);

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "revoke_sessions",
                    "dryRun": true,
                    "userId": parsed.user_id,
                    "sessionId": parsed.session_id,
                    "details": "Would revoke login sessions"
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    format!("DRY RUN: sessions for user {} would be revoked", parsed.user_id),
                ));
            }

            if let Some(sid) = parsed.session_id {
                let session_id = mbl_core::SessionId(sid);
                match services.auth.revoke_session(&caller.actor, session_id).await {
                    Ok(()) => Ok(CallToolResult::structured(
                        &json!({ "revoked": true, "userId": parsed.user_id }),
                        "Session revoked",
                    )),
                    Err(e) => Ok(service_error_to_tool_result(e)),
                }
            } else {
                let count = mbl_db::repo::sessions::SessionsRepo::new(db.clone())
                    .delete_all_for_user(uid)
                    .await
                    .map_err(|e| JsonRpcError::internal(e.to_string()))?;
                Ok(CallToolResult::structured(
                    &json!({ "revokedCount": count, "userId": parsed.user_id }),
                    format!("Revoked {count} sessions for user {}", parsed.user_id),
                ))
            }
        }
        "delete_user" => {
            caller.require(Scope::Users).map_err(crate::server::map_service_error)?;
            let parsed: DeleteUserArgs = serde_json::from_value(args.clone())
                .map_err(|e| JsonRpcError::invalid_params(format!("invalid arguments: {e}")))?;

            if parsed.dry_run.unwrap_or(false) {
                let plan = json!({
                    "action": "delete_user",
                    "dryRun": true,
                    "userId": parsed.user_id,
                    "details": "Would permanently delete user and revoke all sessions"
                });
                return Ok(CallToolResult::structured(
                    &plan,
                    format!("DRY RUN: user {} would be permanently deleted", parsed.user_id),
                ));
            }

            if parsed.confirm != Some(true) {
                return Ok(CallToolResult::error(format!(
                    "confirmation required: set confirm: true to proceed with permanently deleting user {} (destructive operation)",
                    parsed.user_id
                )));
            }

            let uid = UserId(parsed.user_id);
            match services.users.delete(&caller.actor, uid).await {
                Ok(()) => {
                    let out = json!({ "deletedUserId": parsed.user_id, "success": true });
                    Ok(CallToolResult::structured(
                        &out,
                        format!("Successfully deleted user {}", parsed.user_id),
                    ))
                }
                Err(e) => Ok(service_error_to_tool_result(e)),
            }
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown user tool: {name}"))),
    }
}
