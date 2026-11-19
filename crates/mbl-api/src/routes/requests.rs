//! Media Requests, Radarr/Sonarr Bridge & Hardlink Analysis endpoints (M7.20).

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use mbl_core::{ItemId, RequestId};
use mbl_service::{
    ArrConnectionReport, ArrKind, ArrSettings, CreateMediaRequest, MediaRequestRow,
    MissingEpisodeInfo, MusicSearchHit, StorageHardlinkReport, TestArrConnection,
    UpdateArrSettings,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct MusicSearchQuery {
    pub query: String,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListRequestsQuery {
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequestStatusInput {
    pub status: String,
    pub arr_id: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/requests", get(list_requests).post(create_request))
        .route("/api/v1/requests/{id}/status", patch(update_request_status))
        .route("/api/v1/requests/missing-episodes/{showId}", get(inspect_missing_episodes))
        .route("/api/v1/requests/hardlink-analysis", get(analyze_hardlinks))
        .route("/api/v1/requests/arr/{service}", get(get_arr_settings).put(update_arr_settings))
        .route("/api/v1/requests/arr/{service}/test", post(test_arr_connection))
        .route("/api/v1/requests/music/search", get(search_music))
}

#[utoipa::path(
    get,
    path = "/api/v1/requests/music/search",
    params(
        ("query" = String, Query, description = "Artist or album name"),
        ("kind" = String, Query, description = "artist or album")
    ),
    responses(
        (status = 200, description = "Artists or albums found through Lidarr", body = Vec<MusicSearchHit>),
        (status = 503, description = "Lidarr is not connected or did not answer")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn search_music(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<MusicSearchQuery>,
) -> Result<Json<Vec<MusicSearchHit>>, ApiError> {
    Ok(Json(state.services.requests.arr.search_music(&auth.actor, &q.kind, &q.query).await?))
}

#[utoipa::path(
    get,
    path = "/api/v1/requests/arr/{service}",
    params(("service" = String, Path, description = "radarr, sonarr or lidarr")),
    responses(
        (status = 200, description = "Download service settings (the API key is never returned)", body = ArrSettings),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn get_arr_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(service): AxumPath<String>,
) -> Result<Json<ArrSettings>, ApiError> {
    let kind = ArrKind::parse(&service)?;
    Ok(Json(state.services.requests.arr.get_settings(&auth.actor, kind).await?))
}

#[utoipa::path(
    put,
    path = "/api/v1/requests/arr/{service}",
    params(("service" = String, Path, description = "radarr, sonarr or lidarr")),
    request_body = UpdateArrSettings,
    responses(
        (status = 200, description = "Settings saved", body = ArrSettings),
        (status = 422, description = "Missing address, key, root folder or quality profile")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn update_arr_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(service): AxumPath<String>,
    Json(body): Json<UpdateArrSettings>,
) -> Result<Json<ArrSettings>, ApiError> {
    let kind = ArrKind::parse(&service)?;
    let result = state.services.requests.arr.update_settings(&auth.actor, kind, body).await;
    if let Err(e) = &result {
        tracing::warn!(
            service = kind.name(),
            failure = e.code(),
            "download service settings not saved"
        );
    }
    Ok(Json(result?))
}

#[utoipa::path(
    post,
    path = "/api/v1/requests/arr/{service}/test",
    params(("service" = String, Path, description = "radarr, sonarr or lidarr")),
    request_body = TestArrConnection,
    responses(
        (status = 200, description = "Connected; lists root folders and quality profiles", body = ArrConnectionReport),
        (status = 503, description = "The service did not answer or rejected the key")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn test_arr_connection(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(service): AxumPath<String>,
    Json(body): Json<TestArrConnection>,
) -> Result<Json<ArrConnectionReport>, ApiError> {
    let kind = ArrKind::parse(&service)?;
    let result = state.services.requests.arr.test_connection(&auth.actor, kind, body).await;
    if let Err(e) = &result {
        tracing::warn!(
            service = kind.name(),
            failure = e.code(),
            "download service connection test failed"
        );
    }
    Ok(Json(result?))
}

#[utoipa::path(
    get,
    path = "/api/v1/requests",
    params(
        ("status" = Option<String>, Query, description = "Filter by status: pending, approved, available")
    ),
    responses(
        (status = 200, description = "List media requests", body = Vec<MediaRequestRow>),
    ),
    security(("bearer_auth" = []))
)]
async fn list_requests(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListRequestsQuery>,
) -> Result<Json<Vec<MediaRequestRow>>, ApiError> {
    let list = state.services.requests.list_requests(&auth.actor, q.status.as_deref()).await?;
    Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/v1/requests",
    request_body = CreateMediaRequest,
    responses(
        (status = 201, description = "Media request submitted", body = MediaRequestRow),
    ),
    security(("bearer_auth" = []))
)]
async fn create_request(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateMediaRequest>,
) -> Result<(StatusCode, Json<MediaRequestRow>), ApiError> {
    let row = state.services.requests.create_request(&auth.actor, body).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/requests/{id}/status",
    params(
        ("id" = i64, Path, description = "Request ID")
    ),
    request_body = UpdateRequestStatusInput,
    responses(
        (status = 204, description = "Request status updated; approving adds the title to Radarr/Sonarr when turned on"),
        (status = 403, description = "Admin required"),
        (status = 503, description = "Radarr/Sonarr did not add the title; the status is unchanged")
    ),
    security(("bearer_auth" = []))
)]
async fn update_request_status(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Json(body): Json<UpdateRequestStatusInput>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .requests
        .update_status(&auth.actor, RequestId(id), &body.status, body.arr_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/requests/missing-episodes/{showId}",
    params(
        ("showId" = i64, Path, description = "Show Media Item ID")
    ),
    responses(
        (status = 200, description = "Missing episodes detected across seasons", body = Vec<MissingEpisodeInfo>),
    ),
    security(("bearer_auth" = []))
)]
async fn inspect_missing_episodes(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(show_id): AxumPath<i64>,
) -> Result<Json<Vec<MissingEpisodeInfo>>, ApiError> {
    let seasons = state.services.items.children(&auth.actor, ItemId(show_id)).await?;

    let mut report = Vec::new();
    for season in seasons {
        let season_num = season.idx.unwrap_or(1);
        let episodes = state.services.items.children(&auth.actor, season.id).await?;
        let ep_numbers: Vec<i64> = episodes.into_iter().filter_map(|e| e.idx).collect();
        let missing =
            mbl_service::MediaRequestService::find_missing_episodes(season_num, &ep_numbers);
        if !missing.missing_episode_numbers.is_empty() {
            report.push(missing);
        }
    }

    Ok(Json(report))
}

#[utoipa::path(
    get,
    path = "/api/v1/requests/hardlink-analysis",
    responses(
        (status = 200, description = "Hardlink-aware storage analysis", body = StorageHardlinkReport),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn analyze_hardlinks(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<StorageHardlinkReport>, ApiError> {
    auth.actor.require_admin()?;
    let libraries = state.services.libraries.list(&auth.actor).await?;
    let mut file_paths = Vec::new();

    for lib in libraries {
        if let Ok(parts) = mbl_db::repo::media::parts_for_library(&state.db, lib.id).await {
            for p in parts {
                file_paths.push(p.path);
            }
        }
    }

    // Node media is not on this disk; reading metadata of many files blocks, so do it off the runtime.
    file_paths.retain(|p| !p.starts_with("mblnode://"));
    let report = tokio::task::spawn_blocking(move || {
        let path_refs: Vec<&Path> = file_paths.iter().map(Path::new).collect();
        mbl_service::MediaRequestService::analyze_hardlinks(&path_refs)
    })
    .await
    .map_err(|_| ApiError::from(mbl_core::Error::Unavailable("storage analysis failed".into())))?;
    Ok(Json(report))
}
