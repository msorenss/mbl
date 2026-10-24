use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::{JobId, LibraryId, LibraryKind};
use mbl_db::repo::libraries::Library;
use mbl_service::paths::DirListing;
use mbl_service::{CreateLibrary, UpdateLibrary};
use serde::{Deserialize, Serialize};

use crate::extract::{AdminUser, AuthUser};
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/libraries", get(list).post(create))
        .route("/api/v1/libraries/{id}", get(get_one).patch(update).delete(remove))
        .route("/api/v1/libraries/{id}/scan", post(scan))
        .route("/api/v1/system/browse", get(browse))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDto {
    pub id: LibraryId,
    pub name: String,
    pub kind: LibraryKind,
    pub paths: Vec<String>,
    pub created_at: i64,
}

impl From<Library> for LibraryDto {
    fn from(l: Library) -> Self {
        Self { id: l.id, name: l.name, kind: l.kind, paths: l.paths, created_at: l.created_at }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLibraryRequest {
    pub name: String,
    pub kind: LibraryKind,
    pub paths: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLibraryRequest {
    pub name: Option<String>,
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    pub path: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<LibraryDto>>, ApiError> {
    let libs = state.services.libraries.list(&auth.actor).await?;
    Ok(Json(libs.into_iter().map(Into::into).collect()))
}

async fn get_one(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<LibraryId>,
) -> Result<Json<LibraryDto>, ApiError> {
    Ok(Json(state.services.libraries.get(&auth.actor, id).await?.into()))
}

async fn create(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Json(req): Json<CreateLibraryRequest>,
) -> Result<(StatusCode, Json<LibraryDto>), ApiError> {
    let input = CreateLibrary { name: req.name, kind: req.kind, paths: req.paths };
    let lib = state.services.libraries.create(&auth.actor, input).await?;
    Ok((StatusCode::CREATED, Json(lib.into())))
}

async fn update(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<LibraryId>,
    Json(req): Json<UpdateLibraryRequest>,
) -> Result<Json<LibraryDto>, ApiError> {
    let patch = UpdateLibrary { name: req.name, paths: req.paths };
    Ok(Json(state.services.libraries.update(&auth.actor, id, patch).await?.into()))
}

async fn remove(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<LibraryId>,
) -> Result<StatusCode, ApiError> {
    state.services.libraries.delete(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn browse(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Query(q): Query<BrowseQuery>,
) -> Result<Json<DirListing>, ApiError> {
    Ok(Json(state.services.libraries.browse(&auth.actor, q.path).await?))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobAccepted {
    pub job_id: JobId,
}

async fn scan(
    State(state): State<AppState>,
    AdminUser(auth): AdminUser,
    Path(id): Path<LibraryId>,
) -> Result<(StatusCode, Json<JobAccepted>), ApiError> {
    let job_id = state.services.libraries.scan(&auth.actor, id).await?;
    Ok((StatusCode::ACCEPTED, Json(JobAccepted { job_id })))
}
