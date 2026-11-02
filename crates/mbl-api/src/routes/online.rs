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

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OnlineStreamQuery {
    /// Optional session or auth token for players (e.g. Chromecast) that cannot send custom HTTP headers.
    pub token: Option<String>,
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
        .route(
            "/api/v1/online/playback/{id}/index.m3u8",
            get(online_manifest).options(crate::routes::stream::media_preflight),
        )
        .route(
            "/api/v1/online/playback/{id}/resource/{index}",
            get(online_resource).options(crate::routes::stream::media_preflight),
        )
        .layer(axum::middleware::map_response(crate::routes::stream::media_cors))
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
    tracing::info!(target: "mbl::online", user_id = auth.actor.user_id.0, query = %query.q, "WebTV search requested");
    match state.services.online.search(&auth.actor, &query.q).await {
        Ok(res) => {
            tracing::info!(
                target: "mbl::online",
                user_id = auth.actor.user_id.0,
                query = %query.q,
                provider = %res.provider,
                results = res.results.len(),
                indexed = res.indexed.len(),
                warning = ?res.warning,
                "WebTV search completed successfully"
            );
            Ok(Json(res))
        }
        Err(e) => {
            tracing::warn!(target: "mbl::online", user_id = auth.actor.user_id.0, query = %query.q, error = %e, "WebTV search failed");
            Err(e.into())
        }
    }
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
    tracing::info!(target: "mbl::online", user_id = auth.actor.user_id.0, url = %request.url, "WebTV source inspection requested");
    match state.services.online.inspect(&auth.actor, &request.url).await {
        Ok(source) => {
            tracing::info!(
                target: "mbl::online",
                url = %request.url,
                title = %source.title,
                availability = ?source.availability,
                has_media = source.media_url.is_some(),
                detail = %source.detail,
                "WebTV source inspection completed"
            );
            Ok(Json(source))
        }
        Err(e) => {
            tracing::warn!(target: "mbl::online", url = %request.url, error = %e, "WebTV source inspection failed");
            Err(e.into())
        }
    }
}

#[utoipa::path(post,path="/api/v1/online/playback",tag="online",request_body=InspectOnlineSourceRequest,
 responses((status=200,body=mbl_core::online::OnlinePlayback),(status=401),(status=403),(status=422),(status=429),(status=503)),security(("bearer_auth"=[])))]
pub async fn start_online(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
    Json(request): Json<InspectOnlineSourceRequest>,
) -> Result<Json<mbl_core::online::OnlinePlayback>, ApiError> {
    tracing::info!(target: "mbl::online", user_id = auth.actor.user_id.0, url = %request.url, "WebTV playback session requested");
    match state.services.online.start_online(&auth, &request.url).await {
        Ok(playback) => {
            tracing::info!(
                target: "mbl::online",
                session_id = %playback.id,
                stream_url = %playback.stream_url,
                is_hls = playback.hls,
                position = playback.position,
                "WebTV playback session started"
            );
            Ok(Json(playback))
        }
        Err(e) => {
            tracing::warn!(target: "mbl::online", url = %request.url, error = %e, "WebTV playback start failed");
            Err(e.into())
        }
    }
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
    tracing::info!(target: "mbl::online", session_id = %id, user_id = auth.actor.user_id.0, "stopping WebTV playback session");
    state.services.online.stop_online(&auth, &id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn authenticate_online_stream(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    query_token: Option<&str>,
) -> mbl_core::Result<mbl_service::Authenticated> {
    if let Some(tok) = crate::extract::bearer_token(headers) {
        return state.services.auth.authenticate(&tok).await;
    }
    if let Some(tok) = crate::extract::cookie(headers, crate::extract::SESSION_COOKIE) {
        return state.services.auth.authenticate(&tok).await;
    }
    if let Some(tok) = query_token.map(str::trim).filter(|t| !t.is_empty()) {
        return state.services.auth.authenticate(tok).await;
    }
    Err(mbl_core::Error::Unauthorized)
}

#[utoipa::path(
    get,
    path = "/api/v1/online/playback/{id}/index.m3u8",
    tag = "online",
    params(("id" = String, Path), OnlineStreamQuery),
    responses((status = 200, body = String), (status = 401), (status = 403), (status = 404)),
    security(("bearer_auth" = []))
)]
pub async fn online_manifest(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(query): Query<OnlineStreamQuery>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let auth = authenticate_online_stream(&state, &headers, query.token.as_deref()).await?;
    let is_cast = query.token.is_some();
    tracing::info!(
        target: "mbl::online",
        session_id = %id,
        user_id = auth.actor.user_id.0,
        cast = is_cast,
        "serving WebTV HLS manifest"
    );
    let body = state.services.online.online_manifest(&auth, &id, query.token.as_deref()).await?;
    Ok(([("content-type", "application/vnd.apple.mpegurl"), ("cache-control", "no-store")], body))
}

#[utoipa::path(
    get,
    path = "/api/v1/online/playback/{id}/resource/{index}",
    tag = "online",
    params(("id" = String, Path), ("index" = usize, Path), OnlineStreamQuery),
    responses(
        (status = 200, description = "Validated bounded HLS segment or initialization bytes"),
        (status = 401),
        (status = 403),
        (status = 404),
        (status = 429),
        (status = 503)
    ),
    security(("bearer_auth" = []))
)]
pub async fn online_resource(
    State(state): State<AppState>,
    axum::extract::Path((id, index)): axum::extract::Path<(String, usize)>,
    Query(query): Query<OnlineStreamQuery>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let auth = authenticate_online_stream(&state, &headers, query.token.as_deref()).await?;
    let body = state.services.online.online_resource(&auth, &id, index).await?;
    tracing::debug!(
        target: "mbl::online",
        session_id = %id,
        segment_index = index,
        bytes = body.len(),
        "served WebTV HLS resource segment"
    );
    Ok(([("content-type", "application/octet-stream"), ("cache-control", "no-store")], body))
}

#[utoipa::path(get,path="/api/v1/online/history",tag="online",responses((status=200,body=Vec<mbl_core::online::OnlineHistoryEntry>),(status=401),(status=403),(status=503)),security(("bearer_auth"=[])))]
pub async fn online_history(
    AuthUser(auth): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<mbl_core::online::OnlineHistoryEntry>>, ApiError> {
    Ok(Json(state.services.online.online_history(&auth.actor).await?))
}
