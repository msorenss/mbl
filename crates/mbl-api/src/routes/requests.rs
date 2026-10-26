//! Media Requests, Radarr/Sonarr Bridge & Hardlink Analysis endpoints (M7.20).

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use mbl_core::{ItemId, RequestId};
use mbl_service::{CreateMediaRequest, MediaRequestRow, MissingEpisodeInfo, StorageHardlinkReport};
use serde::{Deserialize, Serialize};
use std::path::Path;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

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
        (status = 204, description = "Request status updated"),
        (status = 403, description = "Admin required")
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

    let path_refs: Vec<&Path> = file_paths.iter().map(Path::new).collect();
    let report = mbl_service::MediaRequestService::analyze_hardlinks(&path_refs);
    Ok(Json(report))
}
