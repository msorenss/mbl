//! Online streaming media plugins API routes (M9.6).

use axum::extract::{Path as AxumPath, State};
use axum::routing::get;
use axum::{Json, Router};
use mbl_core::plugins::{PluginChannelFeed, PluginConfig, PluginItem, PluginManifest};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePluginConfigRequest {
    pub enabled: bool,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginStreamUrlResponse {
    pub stream_url: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/plugins", get(list_plugins))
        .route("/api/v1/plugins/{id}", get(get_plugin).patch(update_plugin_config))
        .route("/api/v1/plugins/{id}/channels", get(get_plugin_channels))
        .route("/api/v1/plugins/{id}/channels/{feedId}", get(get_feed_items))
        .route("/api/v1/plugins/{id}/items/{itemId}", get(get_plugin_item))
        .route("/api/v1/plugins/{id}/stream/{itemId}", get(resolve_plugin_stream))
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins",
    responses(
        (status = 200, description = "List installed plugins and manifests", body = Vec<PluginManifest>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_plugins(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PluginManifest>>, ApiError> {
    let manifests = state.services.plugins.list_manifests();
    Ok(Json(manifests))
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}",
    params(("id" = String, Path, description = "Plugin identifier (e.g. youtube, spotify, webdrama)")),
    responses(
        (status = 200, description = "Plugin configuration and status", body = PluginConfig),
        (status = 404, description = "Plugin not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_plugin(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<PluginConfig>, ApiError> {
    let config = state.services.plugins.get_config(&id).await?;
    Ok(Json(config))
}

#[utoipa::path(
    patch,
    path = "/api/v1/plugins/{id}",
    params(("id" = String, Path, description = "Plugin identifier")),
    request_body = UpdatePluginConfigRequest,
    responses(
        (status = 200, description = "Plugin configuration updated", body = PluginConfig),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn update_plugin_config(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdatePluginConfigRequest>,
) -> Result<Json<PluginConfig>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let config = state.services.plugins.update_config(&id, req.enabled, req.settings).await?;
    Ok(Json(config))
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}/channels",
    params(("id" = String, Path, description = "Plugin identifier")),
    responses(
        (status = 200, description = "Available feeds/channels from plugin", body = Vec<PluginChannelFeed>),
    ),
    security(("bearer_auth" = []))
)]
async fn get_plugin_channels(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<PluginChannelFeed>>, ApiError> {
    let feeds = state.services.plugins.get_channel_feeds(&id).await?;
    Ok(Json(feeds))
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}/channels/{feedId}",
    params(
        ("id" = String, Path, description = "Plugin identifier"),
        ("feedId" = String, Path, description = "Feed identifier"),
    ),
    responses(
        (status = 200, description = "Media items in feed", body = Vec<PluginItem>),
    ),
    security(("bearer_auth" = []))
)]
async fn get_feed_items(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((id, feed_id)): AxumPath<(String, String)>,
) -> Result<Json<Vec<PluginItem>>, ApiError> {
    let items = state.services.plugins.get_feed_items(&id, &feed_id).await?;
    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}/items/{itemId}",
    params(
        ("id" = String, Path, description = "Plugin identifier"),
        ("itemId" = String, Path, description = "Item identifier"),
    ),
    responses(
        (status = 200, description = "Plugin item metadata", body = PluginItem),
        (status = 404, description = "Item not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_plugin_item(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((id, item_id)): AxumPath<(String, String)>,
) -> Result<Json<PluginItem>, ApiError> {
    let item = state.services.plugins.get_item(&id, &item_id).await?;
    Ok(Json(item))
}

#[utoipa::path(
    get,
    path = "/api/v1/plugins/{id}/stream/{itemId}",
    params(
        ("id" = String, Path, description = "Plugin identifier"),
        ("itemId" = String, Path, description = "Item identifier"),
    ),
    responses(
        (status = 200, description = "Resolved plugin media stream URL", body = PluginStreamUrlResponse),
        (status = 404, description = "Stream not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn resolve_plugin_stream(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((id, item_id)): AxumPath<(String, String)>,
) -> Result<Json<PluginStreamUrlResponse>, ApiError> {
    let stream_url = state.services.plugins.resolve_stream_url(&id, &item_id).await?;
    Ok(Json(PluginStreamUrlResponse { stream_url }))
}
