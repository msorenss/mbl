//! Browse, detail, search and hubs.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::{Cursor, ItemId, LibraryId, MediaKind, Paginated, PartId};
use mbl_db::repo::items::{ItemDetail, ItemSummary, ListQuery, SortField, SortOrder};
use mbl_service::{Hub, PlaybackPreview, PlaybackProfileInput, PlaybackRequest, SearchGroup};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::extract::AuthUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/libraries/{id}/items", get(list))
        .route("/api/v1/items/{id}", get(detail))
        .route("/api/v1/items/{id}/children", get(children))
        .route(
            "/api/v1/items/{id}/playback-preview",
            get(playback_preview_get).post(playback_preview_post),
        )
        .route("/api/v1/items/{id}/watched", post(mark_watched).delete(mark_unwatched))
        .route("/api/v1/search", get(search))
        .route("/api/v1/hubs/home", get(home))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListParams {
    /// Item kind; defaults to the library's top-level items.
    pub kind: Option<MediaKind>,
    pub sort: Option<SortField>,
    pub order: Option<SortOrder>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub unwatched: Option<bool>,
    pub cursor: Option<String>,
    /// 1–200, default 50.
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    pub q: String,
}

/// Items of a library, paginated.
#[utoipa::path(get, path = "/api/v1/libraries/{id}/items", tag = "browse", operation_id = "list_library_items",
    params(("id" = i64, Path), ListParams),
    responses((status = 200, body = Paginated<ItemSummary>), (status = 404)))]
async fn list(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<LibraryId>,
    Query(p): Query<ListParams>,
) -> Result<Json<Paginated<ItemSummary>>, ApiError> {
    let query = ListQuery {
        kind: p.kind,
        sort: p.sort.unwrap_or_default(),
        order: p.order,
        genre: p.genre,
        year: p.year,
        unwatched: p.unwatched.unwrap_or(false),
        cursor: p.cursor.map(Cursor),
        limit: p.limit.unwrap_or(50),
    };
    Ok(Json(state.services.items.list(&auth.actor, id, &query).await?))
}

/// Item details with versions, streams, people, genres, artwork and watch state.
#[utoipa::path(get, path = "/api/v1/items/{id}", tag = "browse",
    params(("id" = i64, Path)),
    responses((status = 200, body = ItemDetail), (status = 404)))]
async fn detail(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<Json<ItemDetail>, ApiError> {
    Ok(Json(state.services.items.get(&auth.actor, id).await?))
}

/// Seasons, episodes, albums or tracks under an item.
#[utoipa::path(get, path = "/api/v1/items/{id}/children", tag = "browse",
    params(("id" = i64, Path)),
    responses((status = 200, body = Vec<ItemSummary>), (status = 404)))]
async fn children(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Vec<ItemSummary>>, ApiError> {
    Ok(Json(state.services.items.children(&auth.actor, id).await?))
}

/// Title search grouped by kind, at most 10 per group.
#[utoipa::path(get, path = "/api/v1/search", tag = "browse", params(SearchParams),
    responses((status = 200, body = Vec<SearchGroup>)))]
async fn search(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Query(p): Query<SearchParams>,
) -> Result<Json<Vec<SearchGroup>>, ApiError> {
    Ok(Json(state.services.items.search(&auth.actor, &p.q).await?))
}

/// Home screen hubs: Continue Watching, Up Next, Recently Added per library.
#[utoipa::path(get, path = "/api/v1/hubs/home", tag = "browse",
    responses((status = 200, body = Vec<Hub>)))]
async fn home(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<Hub>>, ApiError> {
    Ok(Json(state.services.items.home(&auth.actor).await?))
}

/// Mark an item (or show/season, cascading to episodes) watched.
#[utoipa::path(post, path = "/api/v1/items/{id}/watched", tag = "watch",
    params(("id" = i64, Path)),
    responses((status = 204, description = "Marked watched"), (status = 404)))]
pub async fn mark_watched(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<StatusCode, ApiError> {
    state.services.watch.mark_watched(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Mark an item (or show/season, cascading to episodes) unwatched.
#[utoipa::path(delete, path = "/api/v1/items/{id}/watched", tag = "watch",
    params(("id" = i64, Path)),
    responses((status = 204, description = "Marked unwatched"), (status = 404)))]
pub async fn mark_unwatched(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<StatusCode, ApiError> {
    state.services.watch.mark_unwatched(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlaybackPreviewQuery {
    /// Built-in profile name ("chrome", "firefox", "safari") or serialized JSON profile.
    pub profile: Option<String>,
    pub part_id: Option<PartId>,
    pub max_bitrate: Option<u64>,
    pub audio_stream_index: Option<u32>,
    pub audio_offset_ms: Option<i64>,
    pub subtitle_stream_index: Option<u32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackPreviewRequestBody {
    pub part_id: Option<PartId>,
    #[schema(value_type = Option<Object>)]
    pub profile: Option<PlaybackProfileInput>,
    pub max_bitrate: Option<u64>,
    pub audio_stream_index: Option<u32>,
    pub audio_offset_ms: Option<i64>,
    pub subtitle_stream_index: Option<u32>,
}

/// Preview playback decision (Direct Play vs Direct Stream vs Transcode) without starting a session.
#[utoipa::path(get, path = "/api/v1/items/{id}/playback-preview", tag = "playback", operation_id = "preview_playback_get",
    params(("id" = i64, Path), PlaybackPreviewQuery),
    responses((status = 200, body = PlaybackPreview), (status = 404)))]
async fn playback_preview_get(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
    Query(q): Query<PlaybackPreviewQuery>,
) -> Result<Json<PlaybackPreview>, ApiError> {
    let profile_input = match q.profile {
        Some(s) if s.trim().starts_with('{') => {
            serde_json::from_str::<PlaybackProfileInput>(&s).ok()
        }
        Some(name) => Some(PlaybackProfileInput::Named(name)),
        None => None,
    };
    let preview = state
        .services
        .playback
        .preview(
            &auth.actor,
            PlaybackRequest {
                item_id: id,
                part_id: q.part_id,
                profile: profile_input,
                max_bitrate: q.max_bitrate,
                audio_stream_index: q.audio_stream_index,
                audio_offset_ms: q.audio_offset_ms.unwrap_or(0),
                subtitle_stream_index: q.subtitle_stream_index,
                start_ms: None,
                wait_for_ready: false,
                network: None,
            },
        )
        .await?;
    Ok(Json(preview))
}

/// Preview playback decision with custom dynamic client device profile body.
#[utoipa::path(post, path = "/api/v1/items/{id}/playback-preview", tag = "playback", operation_id = "preview_playback_post",
    params(("id" = i64, Path)),
    request_body = PlaybackPreviewRequestBody,
    responses((status = 200, body = PlaybackPreview), (status = 404)))]
async fn playback_preview_post(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
    Json(body): Json<PlaybackPreviewRequestBody>,
) -> Result<Json<PlaybackPreview>, ApiError> {
    let preview = state
        .services
        .playback
        .preview(
            &auth.actor,
            PlaybackRequest {
                item_id: id,
                part_id: body.part_id,
                profile: body.profile,
                max_bitrate: body.max_bitrate,
                audio_stream_index: body.audio_stream_index,
                audio_offset_ms: body.audio_offset_ms.unwrap_or(0),
                subtitle_stream_index: body.subtitle_stream_index,
                start_ms: None,
                wait_for_ready: false,
                network: None,
            },
        )
        .await?;
    Ok(Json(preview))
}
