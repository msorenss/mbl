//! Public internet title discovery and bounded source inspection.
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, post},
};
use mbl_core::online::{OnlineSearchResponse, OnlineSource};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{ApiError, AppState, extract::AuthUser};

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
}

#[derive(Deserialize, ToSchema)]
pub struct InspectOnlineSourceRequest {
    pub url: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/online/search", get(search_online_sources))
        .route("/api/v1/online/inspect", post(inspect_online_source))
        .route("/api/v1/online/history", get(online_history))
        .route("/api/v1/online/playback", post(start_online))
        .route(
            "/api/v1/online/playback/{id}",
            axum::routing::patch(progress_online).delete(stop_online),
        )
        .route("/api/v1/online/playback/{id}/index.m3u8", get(online_manifest))
        .route("/api/v1/online/playback/{id}/resource/{index}", get(online_resource))
}

#[utoipa::path(get,path="/api/v1/online/search",tag="online",
    params(("q"=String,Query,description="Title to search for across the public web (2–160 characters)")),
    responses((status=200,body=OnlineSearchResponse),(status=401),(status=403,description="Unrated internet content is unavailable to restricted profiles"),(status=422,description="Search titles must contain 2–160 characters"),(status=429),(status=503)),
    security(("bearer_auth"=[])))]
pub async fn search_online_sources(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<OnlineSearchResponse>, ApiError> {
    Ok(Json(state.services.online.search(&auth.actor, &query.q).await?))
}

#[utoipa::path(post,path="/api/v1/online/inspect",tag="online",
    request_body=InspectOnlineSourceRequest,
    responses((status=200,body=OnlineSource),(status=401),(status=403),(status=422,description="Only public HTTP(S) source addresses are allowed"),(status=429),(status=503)),
    security(("bearer_auth"=[])))]
pub async fn inspect_online_source(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    Json(request): Json<InspectOnlineSourceRequest>,
) -> Result<Json<OnlineSource>, ApiError> {
    Ok(Json(state.services.online.inspect(&auth.actor, &request.url).await?))
}

#[utoipa::path(post,path="/api/v1/online/playback",tag="online",request_body=InspectOnlineSourceRequest,
 responses((status=200,body=mbl_core::online::OnlinePlayback),(status=401),(status=403),(status=422),(status=429),(status=503)),security(("bearer_auth"=[])))]
pub async fn start_online(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    Json(request): Json<InspectOnlineSourceRequest>,
) -> Result<Json<mbl_core::online::OnlinePlayback>, ApiError> {
    Ok(Json(state.services.online.start_online(&auth, &request.url).await?))
}
#[utoipa::path(patch,path="/api/v1/online/playback/{id}",tag="online",params(("id"=String,Path)),request_body=mbl_core::online::OnlineProgress,
 responses((status=204),(status=401),(status=403),(status=404),(status=422)),security(("bearer_auth"=[])))]
pub async fn progress_online(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(progress): Json<mbl_core::online::OnlineProgress>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.services.online.online_progress(&auth, &id, progress).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
#[utoipa::path(delete,path="/api/v1/online/playback/{id}",tag="online",params(("id"=String,Path)),responses((status=204),(status=401),(status=403),(status=404)),security(("bearer_auth"=[])))]
pub async fn stop_online(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.services.online.stop_online(&auth, &id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
#[utoipa::path(get,path="/api/v1/online/playback/{id}/index.m3u8",tag="online",params(("id"=String,Path)),responses((status=200,body=String),(status=401),(status=403),(status=404)),security(("bearer_auth"=[])))]
pub async fn online_manifest(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let body = state.services.online.online_manifest(&auth, &id).await?;
    Ok(([("content-type", "application/vnd.apple.mpegurl"), ("cache-control", "no-store")], body))
}
#[utoipa::path(get,path="/api/v1/online/playback/{id}/resource/{index}",tag="online",params(("id"=String,Path),("index"=usize,Path)),responses((status=200,description="Validated bounded HLS segment or initialization bytes"),(status=401),(status=403),(status=404),(status=429),(status=503)),security(("bearer_auth"=[])))]
pub async fn online_resource(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    axum::extract::Path((id, index)): axum::extract::Path<(String, usize)>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let body = state.services.online.online_resource(&auth, &id, index).await?;
    Ok(([("content-type", "application/octet-stream"), ("cache-control", "no-store")], body))
}

#[utoipa::path(get,path="/api/v1/online/history",tag="online",responses((status=200,body=Vec<mbl_core::online::OnlineHistoryEntry>),(status=401),(status=403),(status=503)),security(("bearer_auth"=[])))]
pub async fn online_history(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<mbl_core::online::OnlineHistoryEntry>>, ApiError> {
    Ok(Json(state.services.online.online_history(&auth.actor).await?))
}
