//! Live TV, Tuner, Channel, EPG, Recording and Virtual Channel routes (M9.1 - M9.7).

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::ids::{ChannelId, RecordingId, RecordingRuleId, TunerId, VirtualChannelId};
use mbl_core::livetv::{
    Channel, CreateRecordingRuleRequest, CreateTunerRequest, EpgGridResponse, EpgProgram,
    LiveStreamInfo, Recording, RecordingPlan, RecordingRule, ScheduleRecordingRequest, Tuner,
    UpdateChannelRequest,
};
use mbl_core::plugins::{CreateVirtualChannelRequest, VirtualChannel, VirtualPlayhead};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct EpgGridQuery {
    pub start_time: i64,
    pub end_time: i64,
    #[serde(default)]
    pub include_hidden: bool,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct EpgSearchQuery {
    pub query: String,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    50
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct RecordingsQuery {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportM3uRequest {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportCountResponse {
    pub count: u32,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuccessResponse {
    pub success: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        // Tuners
        .route("/api/v1/livetv/tuners", get(list_tuners).post(add_tuner))
        .route("/api/v1/livetv/tuners/discover", post(discover_tuners))
        .route("/api/v1/livetv/tuners/{id}", get(get_tuner).delete(delete_tuner))
        .route("/api/v1/livetv/tuners/m3u", post(import_m3u))
        // Channels
        .route("/api/v1/livetv/channels", get(list_channels))
        .route(
            "/api/v1/livetv/channels/{id}",
            get(get_channel).patch(update_channel).delete(delete_channel),
        )
        // EPG
        .route("/api/v1/livetv/epg", get(get_epg_grid))
        .route("/api/v1/livetv/epg/search", get(search_guide))
        .route("/api/v1/livetv/epg/xmltv", post(import_xmltv))
        // Live Stream Engine
        .route("/api/v1/livetv/stream/{channelId}", get(resolve_live_stream))
        .route("/api/v1/livetv/stream/{channelId}/release", post(release_live_stream))
        // Recordings
        .route("/api/v1/livetv/recordings", get(list_recordings).post(schedule_recording))
        .route("/api/v1/livetv/recordings/preview", post(preview_recording))
        .route(
            "/api/v1/livetv/recordings/{id}/recovery",
            get(preview_recording_recovery).post(recover_recording),
        )
        .route("/api/v1/livetv/recordings/{id}", get(get_recording).delete(cancel_recording))
        // Series Rules
        .route("/api/v1/livetv/rules", get(list_recording_rules).post(create_series_rule))
        .route("/api/v1/livetv/rules/{id}", get(get_recording_rule).delete(delete_recording_rule))
        // Virtual 24/7 Channels
        .route(
            "/api/v1/livetv/virtual-channels",
            get(list_virtual_channels).post(create_virtual_channel),
        )
        .route(
            "/api/v1/livetv/virtual-channels/{id}",
            get(get_virtual_channel).delete(delete_virtual_channel),
        )
        .route("/api/v1/livetv/virtual-channels/{id}/playhead", get(get_virtual_playhead))
}

// ---------------------------------------------------------------------------
// Tuner Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/livetv/tuners",
    responses(
        (status = 200, description = "List configured tuners", body = Vec<Tuner>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_tuners(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Tuner>>, ApiError> {
    let tuners = state.services.livetv.list_tuners_for(&auth.actor).await?;
    Ok(Json(tuners))
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/tuners",
    request_body = CreateTunerRequest,
    responses(
        (status = 200, description = "Tuner added", body = Tuner),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn add_tuner(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateTunerRequest>,
) -> Result<Json<Tuner>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let tuner = state.services.livetv.add_tuner(req).await?;
    Ok(Json(tuner))
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/tuners/discover",
    responses(
        (status = 200, description = "Discovered tuners on local network", body = Vec<CreateTunerRequest>),
        (status = 403, description = "Admin required"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn discover_tuners(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<CreateTunerRequest>>, ApiError> {
    auth.actor.require_admin()?;
    let discovered = state.services.livetv.discover_hdhomerun().await?;
    Ok(Json(discovered))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/tuners/{id}",
    params(("id" = i64, Path, description = "Tuner ID")),
    responses(
        (status = 200, description = "Tuner details", body = Tuner),
        (status = 404, description = "Tuner not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_tuner(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Tuner>, ApiError> {
    let tuner = state.services.livetv.get_tuner_for(&auth.actor, TunerId(id)).await?;
    Ok(Json(tuner))
}

#[utoipa::path(
    delete,
    path = "/api/v1/livetv/tuners/{id}",
    params(("id" = i64, Path, description = "Tuner ID")),
    responses(
        (status = 200, description = "Tuner deleted", body = SuccessResponse),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_tuner(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<SuccessResponse>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let success = state.services.livetv.delete_tuner(TunerId(id)).await?;
    Ok(Json(SuccessResponse { success }))
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/tuners/m3u",
    request_body = ImportM3uRequest,
    responses(
        (status = 200, description = "M3U playlist imported", body = Tuner),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn import_m3u(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<ImportM3uRequest>,
) -> Result<Json<Tuner>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let tuner_id = state.services.livetv.import_m3u(&req.name, &req.content).await?;
    let tuner = state.services.livetv.get_tuner(tuner_id).await?;
    Ok(Json(tuner))
}

// ---------------------------------------------------------------------------
// Channel Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/livetv/channels",
    params(("includeHidden" = Option<bool>, Query, description = "Include hidden channels")),
    responses(
        (status = 200, description = "List all channels", body = Vec<Channel>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_channels(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<Channel>>, ApiError> {
    let include_hidden =
        params.get("includeHidden").map(|v| v == "true" || v == "1").unwrap_or(false);
    let channels = state.services.livetv.list_channels_for(&auth.actor, include_hidden).await?;
    Ok(Json(channels))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/channels/{id}",
    params(("id" = i64, Path, description = "Channel ID")),
    responses(
        (status = 200, description = "Channel details", body = Channel),
        (status = 404, description = "Channel not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_channel(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Channel>, ApiError> {
    let channel = state.services.livetv.get_channel_for(&auth.actor, ChannelId(id)).await?;
    Ok(Json(channel))
}

#[utoipa::path(
    patch,
    path = "/api/v1/livetv/channels/{id}",
    params(("id" = i64, Path, description = "Channel ID")),
    request_body = UpdateChannelRequest,
    responses(
        (status = 200, description = "Channel updated", body = Channel),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn update_channel(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<Channel>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    state.services.livetv.update_channel(ChannelId(id), req).await?;
    let channel = state.services.livetv.get_channel_for(&auth.actor, ChannelId(id)).await?;
    Ok(Json(channel))
}

#[utoipa::path(
    delete,
    path = "/api/v1/livetv/channels/{id}",
    params(("id" = i64, Path, description = "Channel ID")),
    responses(
        (status = 200, description = "Channel deleted", body = SuccessResponse),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_channel(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<SuccessResponse>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let success = state.services.livetv.delete_channel(ChannelId(id)).await?;
    Ok(Json(SuccessResponse { success }))
}

// ---------------------------------------------------------------------------
// EPG Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/livetv/epg",
    params(EpgGridQuery),
    responses(
        (status = 200, description = "EPG 2-axis grid response", body = EpgGridResponse),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_epg_grid(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<EpgGridQuery>,
) -> Result<Json<EpgGridResponse>, ApiError> {
    let grid = state
        .services
        .livetv
        .get_epg_grid_for(&auth.actor, query.start_time, query.end_time, query.include_hidden)
        .await?;
    Ok(Json(grid))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/epg/search",
    params(EpgSearchQuery),
    responses(
        (status = 200, description = "Search programs across guide", body = Vec<EpgProgram>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn search_guide(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<EpgSearchQuery>,
) -> Result<Json<Vec<EpgProgram>>, ApiError> {
    let programs = state
        .services
        .livetv
        .search_guide_for(&auth.actor, &query.query, query.start_time, query.end_time, query.limit)
        .await?;
    Ok(Json(programs))
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/epg/xmltv",
    responses(
        (status = 200, description = "XMLTV programs imported", body = ImportCountResponse),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn import_xmltv(
    auth: AuthUser,
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<ImportCountResponse>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let count = state.services.livetv.import_xmltv(&body).await?;
    Ok(Json(ImportCountResponse { count }))
}

// ---------------------------------------------------------------------------
// Live Stream Engine Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/livetv/stream/{channelId}",
    params(("channelId" = i64, Path, description = "Channel ID"), ("sessionId" = Option<String>, Query, description = "Renew an existing live lease")),
    responses(
        (status = 200, description = "Ready server-owned live HLS relay with a 45-second renewable lease", body = LiveStreamInfo),
        (status = 404, description = "Channel or lease not found"),
        (status = 403, description = "Current programme is unavailable to this profile"),
        (status = 409, description = "Tuner concurrency limit reached"),
        (status = 503, description = "Encoder capacity, source or live HLS startup unavailable"),
    ),
    security(("bearer_auth" = []))
)]
async fn resolve_live_stream(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(channel_id): AxumPath<i64>,
    axum::extract::Query(lease): axum::extract::Query<LiveRenewQuery>,
) -> Result<axum::response::Response, ApiError> {
    let info = state
        .services
        .livetv
        .resolve_live_stream(auth.session_id, ChannelId(channel_id), lease.session_id.as_deref())
        .await?;
    let mut response = axum::response::IntoResponse::into_response(Json(info));
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveRenewQuery {
    session_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveLeaseQuery {
    session_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/stream/{channelId}/release",
    params(("channelId" = i64, Path, description = "Channel ID"), ("sessionId" = String, Query, description = "Owned live lease ID")),
    responses(
        (status = 200, description = "Stream released and tuner freed", body = SuccessResponse),
    ),
    security(("bearer_auth" = []))
)]
async fn release_live_stream(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(channel_id): AxumPath<i64>,
    axum::extract::Query(lease): axum::extract::Query<LiveLeaseQuery>,
) -> Result<Json<SuccessResponse>, ApiError> {
    state
        .services
        .livetv
        .release_live_stream(auth.session_id, ChannelId(channel_id), &lease.session_id)
        .await?;
    Ok(Json(SuccessResponse { success: true }))
}

// ---------------------------------------------------------------------------
// Recordings & Series Rules Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/livetv/recordings",
    params(RecordingsQuery),
    responses(
        (status = 200, description = "List recordings", body = Vec<Recording>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_recordings(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<RecordingsQuery>,
) -> Result<Json<Vec<Recording>>, ApiError> {
    let recordings =
        state.services.livetv.list_recordings_for(&auth.actor, query.status.as_deref()).await?;
    Ok(Json(recordings))
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/recordings",
    request_body = ScheduleRecordingRequest,
    responses(
        (status = 200, description = "Recording scheduled", body = Recording),
        (status = 409, description = "Tuner conflict"),
        (status = 404, description = "Programme or DVR library unavailable to this profile"),
        (status = 403, description = "Age-limited profiles require an EPG programme"),
        (status = 422, description = "Invalid or expired recording window"),
    ),
    security(("bearer_auth" = []))
)]
async fn schedule_recording(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<ScheduleRecordingRequest>,
) -> Result<Json<Recording>, ApiError> {
    let rec = state.services.livetv.schedule_recording(&auth.actor, req).await?;
    Ok(Json(rec))
}

#[utoipa::path(post, path="/api/v1/livetv/recordings/preview", request_body=ScheduleRecordingRequest,
    responses((status=200, description="Read-only canonical recording plan and current resource snapshot", body=RecordingPlan),
        (status=403, description="Retry is not owned or programme is not allowed"),
        (status=404, description="Source or recording is unavailable"),
        (status=409, description="Recording cannot be retried"),
        (status=422, description="Invalid recording window")), security(("bearer_auth"=[])))]
pub async fn preview_recording(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<ScheduleRecordingRequest>,
) -> Result<Json<RecordingPlan>, ApiError> {
    Ok(Json(state.services.livetv.preview_recording(&auth.actor, req).await?))
}

#[utoipa::path(get,path="/api/v1/livetv/recordings/{id}/recovery",params(("id"=i64,Path)),
    responses((status=200,body=mbl_core::livetv::RecordingRecoveryPlan),(status=403,description="Owner or administrator required; current owner profile must allow the archive"),(status=404),(status=409)),security(("bearer_auth"=[])))]
pub async fn preview_recording_recovery(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<mbl_core::livetv::RecordingRecoveryPlan>, ApiError> {
    Ok(Json(state.services.livetv.preview_recording_recovery(&auth.actor, RecordingId(id)).await?))
}

#[utoipa::path(post,path="/api/v1/livetv/recordings/{id}/recovery",params(("id"=i64,Path)),
    responses((status=200,body=Recording),(status=403,description="Owner or administrator required; current owner profile must allow the archive"),(status=404),(status=409),(status=422,description="Saved media cannot be recovered"),(status=503,description="Recovery storage unavailable")),security(("bearer_auth"=[])))]
pub async fn recover_recording(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Recording>, ApiError> {
    Ok(Json(state.services.livetv.recover_recording(&auth.actor, RecordingId(id)).await?))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/recordings/{id}",
    params(("id" = i64, Path, description = "Recording ID")),
    responses(
        (status = 200, description = "Recording details", body = Recording),
        (status = 404, description = "Recording not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_recording(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Recording>, ApiError> {
    let rec = state.services.livetv.get_recording_for(&auth.actor, RecordingId(id)).await?;
    Ok(Json(rec))
}

#[utoipa::path(
    delete,
    path = "/api/v1/livetv/recordings/{id}",
    params(("id" = i64, Path, description = "Recording ID")),
    responses(
        (status = 200, description = "Recording cancelled or deleted", body = SuccessResponse),
        (status = 403, description = "Only the owner or an administrator may delete a shared recording"),
        (status = 404, description = "Recording not found or inaccessible"),
    ),
    security(("bearer_auth" = []))
)]
async fn cancel_recording(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<SuccessResponse>, ApiError> {
    state.services.livetv.get_recording_for(&auth.actor, RecordingId(id)).await?;
    let success = state.services.livetv.cancel_recording(&auth.actor, RecordingId(id)).await?;
    Ok(Json(SuccessResponse { success }))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/rules",
    responses(
        (status = 200, description = "List series recording rules", body = Vec<RecordingRule>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_recording_rules(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<RecordingRule>>, ApiError> {
    let rules = state.services.livetv.list_recording_rules(&auth.actor).await?;
    Ok(Json(rules))
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/rules",
    request_body = CreateRecordingRuleRequest,
    responses(
        (status = 200, description = "Series recording rule created", body = RecordingRule),
        (status = 404, description = "Channel or recording library inaccessible"),
        (status = 422, description = "Invalid recording or retention policy"),
    ),
    security(("bearer_auth" = []))
)]
async fn create_series_rule(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateRecordingRuleRequest>,
) -> Result<Json<RecordingRule>, ApiError> {
    let rule = state.services.livetv.create_series_rule(&auth.actor, req).await?;
    Ok(Json(rule))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/rules/{id}",
    params(("id" = i64, Path, description = "Rule ID")),
    responses(
        (status = 200, description = "Rule details", body = RecordingRule),
        (status = 404, description = "Rule not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_recording_rule(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<RecordingRule>, ApiError> {
    let rule = state.services.livetv.get_recording_rule(&auth.actor, RecordingRuleId(id)).await?;
    Ok(Json(rule))
}

#[utoipa::path(
    delete,
    path = "/api/v1/livetv/rules/{id}",
    params(("id" = i64, Path, description = "Rule ID")),
    responses(
        (status = 200, description = "Rule deleted", body = SuccessResponse),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_recording_rule(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let rule = state.services.livetv.get_recording_rule(&auth.actor, RecordingRuleId(id)).await?;
    if !auth.actor.is_admin && rule.user_id != auth.actor.user_id {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let success =
        state.services.livetv.delete_recording_rule(&auth.actor, RecordingRuleId(id)).await?;
    Ok(Json(SuccessResponse { success }))
}

// ---------------------------------------------------------------------------
// Virtual 24/7 Channel Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/livetv/virtual-channels",
    responses(
        (status = 200, description = "List virtual linear channels", body = Vec<VirtualChannel>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_auth" = []))
)]
async fn list_virtual_channels(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<VirtualChannel>>, ApiError> {
    let channels = state.services.livetv.list_virtual_channels().await?;
    Ok(Json(channels))
}

#[utoipa::path(
    post,
    path = "/api/v1/livetv/virtual-channels",
    request_body = CreateVirtualChannelRequest,
    responses(
        (status = 200, description = "Virtual channel created", body = VirtualChannel),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn create_virtual_channel(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateVirtualChannelRequest>,
) -> Result<Json<VirtualChannel>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let vc = state.services.livetv.create_virtual_channel(req).await?;
    Ok(Json(vc))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/virtual-channels/{id}",
    params(("id" = i64, Path, description = "Virtual Channel ID")),
    responses(
        (status = 200, description = "Virtual channel details", body = VirtualChannel),
        (status = 404, description = "Not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_virtual_channel(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<VirtualChannel>, ApiError> {
    let vc = state.services.livetv.get_virtual_channel(VirtualChannelId(id)).await?;
    Ok(Json(vc))
}

#[utoipa::path(
    delete,
    path = "/api/v1/livetv/virtual-channels/{id}",
    params(("id" = i64, Path, description = "Virtual Channel ID")),
    responses(
        (status = 200, description = "Virtual channel deleted", body = SuccessResponse),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer_auth" = []))
)]
async fn delete_virtual_channel(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<SuccessResponse>, ApiError> {
    if !auth.actor.is_admin {
        return Err(ApiError::from(mbl_core::Error::Forbidden));
    }
    let success = state.services.livetv.delete_virtual_channel(VirtualChannelId(id)).await?;
    Ok(Json(SuccessResponse { success }))
}

#[utoipa::path(
    get,
    path = "/api/v1/livetv/virtual-channels/{id}/playhead",
    params(("id" = i64, Path, description = "Virtual Channel ID")),
    responses(
        (status = 200, description = "Active playhead calculated on the fly (zero background CPU)", body = VirtualPlayhead),
        (status = 404, description = "Channel not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_virtual_playhead(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<VirtualPlayhead>, ApiError> {
    let now = mbl_core::time::now_ms();
    let playhead = state.services.livetv.get_virtual_playhead(VirtualChannelId(id), now).await?;
    Ok(Json(playhead))
}
