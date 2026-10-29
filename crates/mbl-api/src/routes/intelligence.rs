//! M5 REST endpoints:
//! - Optimized versions (`POST /api/v1/items/{id}/optimize`)
//! - Trickplay sprite manifest & JPEG sheets (`GET /api/v1/stream/trickplay/{partId}/index.json`, `/{sprite}`)
//! - Intro & credits markers (`GET /api/v1/items/{id}/markers`, `POST /api/v1/seasons/{id}/detect-intros`)
//! - Quick-Connect TV login (`POST /api/v1/auth/quick-connect`, `/authorize`, `/poll`)
//! - Hailo NPU AI (`POST /api/v1/items/{id}/index-scenes`, `GET /api/v1/ai/search-scenes`, `GET /api/v1/ai/recap/{id}`, `GET /api/v1/ai/benchmark`)

use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_analysis::TrickplayManifest;
use mbl_core::{ItemId, PartId};
use mbl_db::repo::m5::{MarkerRow, SceneEmbeddingRow};
use mbl_hailo::{HailoEmbeddingBenchmark, SpoilerFreeRecap};
use mbl_service::{
    CreateOptimizedRequest, CreateOptimizedResponse, IndexScenesRequest,
    QuickConnectAuthorizeRequest, QuickConnectInitRequest, QuickConnectInitResponse,
    QuickConnectPollRequest, QuickConnectPollResponse, SceneSearchHit,
};
use serde::Deserialize;

use crate::extract::AuthUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/items/{id}/optimize", post(optimize_item))
        .route("/api/v1/items/{id}/markers", get(get_markers))
        .route("/api/v1/seasons/{id}/detect-intros", post(detect_season_intros))
        .route("/api/v1/stream/trickplay/{partId}/index.json", get(trickplay_manifest))
        .route("/api/v1/stream/trickplay/{partId}/{sprite}", get(trickplay_sprite))
        .route("/api/v1/auth/quick-connect", post(quick_connect_init))
        .route("/api/v1/auth/quick-connect/authorize", post(quick_connect_authorize))
        .route("/api/v1/auth/quick-connect/poll", post(quick_connect_poll))
        .route("/api/v1/items/{id}/index-scenes", post(index_item_scenes))
        .route("/api/v1/ai/search-scenes", get(search_scenes))
        .route("/api/v1/ai/recap/{id}", get(spoiler_free_recap))
        .route("/api/v1/ai/benchmark", get(hailo_benchmark))
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeItemBody {
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default)]
    pub night_window_only: bool,
    #[serde(default)]
    pub dry_run: bool,
}

fn default_profile() -> String {
    "universal_1080p".into()
}

/// Create or preview (`dry_run`) an optimized H.264 + AAC MP4 version for an item (M5.4).
#[utoipa::path(
    post,
    path = "/api/v1/items/{id}/optimize",
    tag = "items",
    params(("id" = i64, Path, description = "Media item ID")),
    request_body = OptimizeItemBody,
    responses(
        (status = 200, body = CreateOptimizedResponse),
        (status = 401),
        (status = 403),
        (status = 404)
    )
)]
pub(crate) async fn optimize_item(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<OptimizeItemBody>,
) -> Result<Json<CreateOptimizedResponse>, ApiError> {
    let res = state
        .services
        .intelligence
        .create_optimized(
            &auth.actor,
            CreateOptimizedRequest {
                item_id: Some(ItemId(id)),
                library_id: None,
                profile: body.profile,
                night_window_only: body.night_window_only,
                dry_run: body.dry_run,
            },
        )
        .await?;
    Ok(Json(res))
}

/// List intro, credits, and chapter markers for an item (M5.6, M5.7).
#[utoipa::path(
    get,
    path = "/api/v1/items/{id}/markers",
    tag = "items",
    params(("id" = i64, Path, description = "Media item ID")),
    responses(
        (status = 200, body = Vec<MarkerRow>),
        (status = 401),
        (status = 404)
    )
)]
pub(crate) async fn get_markers(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Vec<MarkerRow>>, ApiError> {
    let markers = state.services.intelligence.list_markers(&auth.actor, ItemId(id)).await?;
    Ok(Json(markers))
}

/// Run Chromaprint-style audio intro detection and credits detection across all episodes in a season (M5.6, M5.7).
#[utoipa::path(
    post,
    path = "/api/v1/seasons/{id}/detect-intros",
    tag = "items",
    params(("id" = i64, Path, description = "Season item ID")),
    responses(
        (status = 200, body = Vec<MarkerRow>),
        (status = 401),
        (status = 403),
        (status = 404)
    )
)]
pub(crate) async fn detect_season_intros(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Vec<MarkerRow>>, ApiError> {
    let markers = state.services.intelligence.detect_season_intros(&auth.actor, ItemId(id)).await?;
    Ok(Json(markers))
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TrickplayPending {
    pub status: String,
}

/// Get the trickplay sprite sheet manifest (`10x10` 320px tiles every 10s) for a media part (M5.5).
#[utoipa::path(
    get,
    path = "/api/v1/stream/trickplay/{partId}/index.json",
    tag = "stream",
    params(("partId" = i64, Path, description = "Media part ID")),
    responses(
        (status = 200, body = TrickplayManifest),
        (status = 202, body = TrickplayPending, description = "Previews queued for idle generation; retry later"),
        (status = 503, description = "Preview generation failed"),
        (status = 401),
        (status = 404)
    )
)]
pub(crate) async fn trickplay_manifest(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(part_id): Path<i64>,
) -> Result<Response, ApiError> {
    let manifest =
        state.services.intelligence.trickplay_manifest(&auth.actor, PartId(part_id)).await?;
    let mut response = match manifest {
        Some(manifest) => Json(manifest).into_response(),
        None => (
            StatusCode::ACCEPTED,
            [(header::RETRY_AFTER, "5")],
            Json(TrickplayPending { status: "queued".into() }),
        )
            .into_response(),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

/// Serve a trickplay 10x10 JPEG sprite sheet (`sprite_000.jpg`) (M5.5).
#[utoipa::path(
    get,
    path = "/api/v1/stream/trickplay/{partId}/{sprite}",
    tag = "stream",
    params(
        ("partId" = i64, Path, description = "Media part ID"),
        ("sprite" = String, Path, description = "Sprite file name (e.g. sprite_000.jpg)")
    ),
    responses(
        (status = 200, content_type = "image/jpeg"),
        (status = 401),
        (status = 404)
    )
)]
pub(crate) async fn trickplay_sprite(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path((part_id, sprite)): Path<(i64, String)>,
) -> Result<Response, ApiError> {
    let path = state
        .services
        .intelligence
        .trickplay_sprite_path(&auth.actor, PartId(part_id), &sprite)
        .await?;
    let bytes = tokio::fs::read(&path).await.map_err(mbl_core::Error::internal)?;
    let mut resp = (StatusCode::OK, bytes).into_response();
    resp.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(resp)
}

/// Initiate a 6-digit Quick-Connect code flow for `mbl-tv` login (M5.8).
#[utoipa::path(
    post,
    path = "/api/v1/auth/quick-connect",
    tag = "auth",
    security(),
    request_body = QuickConnectInitRequest,
    responses((status = 200, body = QuickConnectInitResponse))
)]
pub(crate) async fn quick_connect_init(
    State(state): State<AppState>,
    Json(req): Json<QuickConnectInitRequest>,
) -> Result<Json<QuickConnectInitResponse>, ApiError> {
    let res = state.services.intelligence.quick_connect_init(req).await?;
    Ok(Json(res))
}

/// Authorize a 6-digit Quick-Connect TV code using the caller's active session (M5.8).
#[utoipa::path(
    post,
    path = "/api/v1/auth/quick-connect/authorize",
    tag = "auth",
    request_body = QuickConnectAuthorizeRequest,
    responses((status = 204), (status = 401), (status = 404), (status = 409))
)]
pub(crate) async fn quick_connect_authorize(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Json(req): Json<QuickConnectAuthorizeRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.intelligence.quick_connect_authorize(&auth.actor, &req.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Poll whether a Quick-Connect secret has been authorized by a user (M5.8).
#[utoipa::path(
    post,
    path = "/api/v1/auth/quick-connect/poll",
    tag = "auth",
    security(),
    request_body = QuickConnectPollRequest,
    responses((status = 200, body = QuickConnectPollResponse), (status = 404))
)]
pub(crate) async fn quick_connect_poll(
    State(state): State<AppState>,
    Json(req): Json<QuickConnectPollRequest>,
) -> Result<Json<QuickConnectPollResponse>, ApiError> {
    let res = state.services.intelligence.quick_connect_poll(&req.secret).await?;
    Ok(Json(res))
}

/// Index visual scenes and tags for a media item using the Hailo NPU multimodal engine (M5.13).
#[utoipa::path(
    post,
    path = "/api/v1/items/{id}/index-scenes",
    tag = "ai",
    params(("id" = i64, Path, description = "Media item ID")),
    request_body = IndexScenesRequest,
    responses((status = 200, body = Vec<SceneEmbeddingRow>), (status = 401), (status = 404))
)]
pub(crate) async fn index_item_scenes(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<IndexScenesRequest>,
) -> Result<Json<Vec<SceneEmbeddingRow>>, ApiError> {
    let rows = state.services.intelligence.index_item_scenes(&auth.actor, ItemId(id), req).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SearchScenesQuery {
    pub q: String,
    #[serde(default = "default_scene_limit")]
    pub limit: usize,
}

fn default_scene_limit() -> usize {
    10
}

/// Natural-language semantic visual scene search across indexed movies and episodes (M5.13).
#[utoipa::path(
    get,
    path = "/api/v1/ai/search-scenes",
    tag = "ai",
    params(SearchScenesQuery),
    responses((status = 200, body = Vec<SceneSearchHit>), (status = 401))
)]
pub(crate) async fn search_scenes(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Query(query): Query<SearchScenesQuery>,
) -> Result<Json<Vec<SceneSearchHit>>, ApiError> {
    let hits =
        state.services.intelligence.search_scenes(&auth.actor, &query.q, query.limit).await?;
    Ok(Json(hits))
}

/// Generate a spoiler-free "Previously On..." recap strictly from the caller's watched episodes (M5.13).
#[utoipa::path(
    get,
    path = "/api/v1/ai/recap/{id}",
    tag = "ai",
    params(("id" = i64, Path, description = "Show or Season item ID")),
    responses((status = 200, body = SpoilerFreeRecap), (status = 401), (status = 404))
)]
pub(crate) async fn spoiler_free_recap(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<SpoilerFreeRecap>, ApiError> {
    let recap = state.services.intelligence.spoiler_free_recap(&auth.actor, ItemId(id)).await?;
    Ok(Json(recap))
}

/// Run a semantic frame embedding benchmark on the Hailo NPU / CLIP engine (M5.13).
#[utoipa::path(
    get,
    path = "/api/v1/ai/benchmark",
    tag = "ai",
    responses((status = 200, body = HailoEmbeddingBenchmark), (status = 401))
)]
pub(crate) async fn hailo_benchmark(
    State(state): State<AppState>,
    AuthUser(_auth): AuthUser,
) -> Json<HailoEmbeddingBenchmark> {
    Json(state.services.intelligence.benchmark_hailo(25))
}
