//! Worker API endpoints (M8.1–M8.9): pairing, status, heartbeats, source streaming,
//! segment uploads, and Wake-on-LAN dispatch.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use mbl_core::{
    Actor, Error, PartId, UserId, WorkerHeartbeat, WorkerId, WorkerJobReport, WorkerPairRequest,
    WorkerPairResponse, WorkerPollRequest, WorkerPollResponse,
};
use mbl_db::repo::m8::WorkerRow;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairingCodeResponse {
    pub code: String,
    pub expires_in_secs: u32,
}

#[derive(Debug, Deserialize)]
pub struct SourceQuery {
    pub token: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/workers/pairing-code", post(generate_worker_pairing_code))
        .route("/api/v1/workers/pair", post(pair_worker))
        .route("/api/v1/workers", get(list_workers))
        .route("/api/v1/workers/{id}", get(get_worker).delete(delete_worker))
        .route("/api/v1/workers/{id}/wake", post(wake_worker))
        .route("/api/v1/workers/heartbeat", post(heartbeat))
        .route("/api/v1/stream/worker/source/{partId}", get(stream_source))
        .route("/api/v1/workers/poll", post(poll_jobs))
        .route("/api/v1/workers/jobs/{jobId}/report", post(report_job))
        .route(
            "/api/v1/workers/jobs/{jobId}/files/{rendition}/{filename}",
            put(upload_job_file)
                .layer(DefaultBodyLimit::max(mbl_service::workers::MAX_WORKER_UPLOAD_BYTES)),
        )
}

#[utoipa::path(
    post,
    path = "/api/v1/workers/pairing-code",
    responses(
        (status = 200, description = "Pairing code generated", body = PairingCodeResponse),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn generate_worker_pairing_code(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<PairingCodeResponse>, ApiError> {
    let code = state.services.workers.generate_pairing_code(&auth.actor).await?;
    Ok(Json(PairingCodeResponse { code, expires_in_secs: 900 }))
}

#[utoipa::path(
    post,
    path = "/api/v1/workers/pair",
    request_body = WorkerPairRequest,
    responses(
        (status = 200, description = "Worker successfully paired", body = WorkerPairResponse),
        (status = 401, description = "Invalid or expired pairing code"),
    )
)]
async fn pair_worker(
    State(state): State<AppState>,
    Json(req): Json<WorkerPairRequest>,
) -> Result<Json<WorkerPairResponse>, ApiError> {
    let resp = state.services.workers.pair_worker(req).await?;
    Ok(Json(resp))
}

#[utoipa::path(
    get,
    path = "/api/v1/workers",
    responses(
        (status = 200, description = "List registered workers", body = Vec<WorkerRow>),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_workers(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkerRow>>, ApiError> {
    let workers = state.services.workers.list_workers(&auth.actor).await?;
    Ok(Json(workers))
}

#[utoipa::path(
    get,
    path = "/api/v1/workers/{id}",
    params(("id" = i64, Path)),
    responses(
        (status = 200, description = "Worker details", body = WorkerRow),
        (status = 404, description = "Worker not found"),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_worker(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<WorkerId>,
) -> Result<Json<WorkerRow>, ApiError> {
    let worker = state.services.workers.get_worker(&auth.actor, id).await?;
    Ok(Json(worker))
}

#[utoipa::path(
    delete,
    path = "/api/v1/workers/{id}",
    params(("id" = i64, Path)),
    responses(
        (status = 204, description = "Worker revoked"),
        (status = 404, description = "Worker not found"),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_worker(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<WorkerId>,
) -> Result<StatusCode, ApiError> {
    state.services.workers.delete_worker(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/workers/{id}/wake",
    params(("id" = i64, Path)),
    responses(
        (status = 204, description = "Wake-on-LAN magic packet transmitted"),
        (status = 404, description = "Worker not found"),
        (status = 403, description = "Forbidden (Admin only)"),
    ),
    security(("bearer_auth" = []))
)]
async fn wake_worker(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<WorkerId>,
) -> Result<StatusCode, ApiError> {
    state.services.workers.wake_worker(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/workers/heartbeat",
    request_body = WorkerHeartbeat,
    responses(
        (status = 204, description = "Heartbeat accepted"),
        (status = 401, description = "Unauthorized worker"),
    ),
    security(("worker_bearer_auth" = []))
)]
async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(hb): Json<WorkerHeartbeat>,
) -> Result<StatusCode, ApiError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;

    let worker = state.services.workers.authenticate(auth_header).await?;
    if worker.id != hb.worker_id {
        return Err(Error::Unauthorized.into());
    }

    state.services.workers.record_heartbeat(hb).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/stream/worker/source/{partId}",
    params(
        ("partId" = i64, Path),
        ("token" = String, Query, description = "Signed source media access token")
    ),
    responses(
        (status = 200, description = "Source file stream"),
        (status = 206, description = "Partial range byte stream"),
        (status = 401, description = "Invalid or expired token"),
        (status = 404, description = "Media part not found")
    )
)]
async fn stream_source(
    State(state): State<AppState>,
    method: Method,
    AxumPath(part_id): AxumPath<PartId>,
    Query(query): Query<SourceQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // Verify worker signed token
    let _worker_id = state.services.workers.verify_source_token(&query.token, part_id)?;

    // Resolve part path
    let admin_actor = Actor::admin(UserId(1));
    let resolved = state.services.stream.resolve_part(&admin_actor, part_id).await?;

    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let is_head = method == Method::HEAD;

    Ok(state.services.stream.serve_part(&resolved, range_header, is_head).await?)
}

async fn authenticate_worker(state: &AppState, headers: &HeaderMap) -> Result<WorkerRow, ApiError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;
    Ok(state.services.workers.authenticate(auth_header).await?)
}

/// Long-poll for transcode assignments. The worker reports its proven encoders and the jobs it is
/// still running; the response arrives as soon as there is work, or empty after about 25 seconds.
#[utoipa::path(
    post,
    path = "/api/v1/workers/poll",
    request_body = WorkerPollRequest,
    responses(
        (status = 200, description = "Assignments and control messages", body = WorkerPollResponse),
        (status = 401, description = "Unauthorized worker"),
    ),
    security(("worker_bearer_auth" = []))
)]
async fn poll_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WorkerPollRequest>,
) -> Result<Json<WorkerPollResponse>, ApiError> {
    let worker = authenticate_worker(&state, &headers).await?;
    let response =
        state.services.workers.poll(&worker, &req, std::time::Duration::from_secs(25)).await?;
    Ok(Json(response))
}

/// Report how a transcode job ended.
#[utoipa::path(
    post,
    path = "/api/v1/workers/jobs/{jobId}/report",
    params(("jobId" = String, Path)),
    request_body = WorkerJobReport,
    responses(
        (status = 204, description = "Report accepted"),
        (status = 401, description = "Unauthorized worker"),
        (status = 404, description = "Unknown job"),
    ),
    security(("worker_bearer_auth" = []))
)]
async fn report_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(job_id): AxumPath<String>,
    Json(report): Json<WorkerJobReport>,
) -> Result<StatusCode, ApiError> {
    let worker = authenticate_worker(&state, &headers).await?;
    state.services.workers.report_job(&worker, &job_id, &report)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Upload one output file of a running job: `init.mp4`, `<n>.m4s` or the EVENT playlist `ffmpeg.m3u8`.
#[utoipa::path(
    put,
    path = "/api/v1/workers/jobs/{jobId}/files/{rendition}/{filename}",
    params(("jobId" = String, Path), ("rendition" = String, Path), ("filename" = String, Path)),
    responses(
        (status = 204, description = "File stored"),
        (status = 401, description = "Unauthorized worker"),
        (status = 404, description = "Job is not running"),
        (status = 422, description = "Invalid file name or rendition"),
    ),
    security(("worker_bearer_auth" = []))
)]
async fn upload_job_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((job_id, rendition, filename)): AxumPath<(String, String, String)>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let worker = authenticate_worker(&state, &headers).await?;
    state.services.workers.store_job_file(&worker, &job_id, &rendition, &filename, &body).await?;
    Ok(StatusCode::NO_CONTENT)
}
