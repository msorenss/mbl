//! Direct-play and HLS streaming endpoints.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, header};
use axum::response::Response;
use axum::routing::get;
use mbl_core::{Actor, Error, PartId};
use mbl_stream::hls::{
    MIME_M4S, MIME_MP4, parse_segment_filename, serve_playlist, serve_segment_file,
};
use mbl_stream::subtitles::{parse_subtitle_target, serve_subtitle_file};
use serde::Deserialize;

use crate::extract::{SESSION_COOKIE, bearer_token, cookie};
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/stream/direct/{partId}", get(direct))
        .route("/api/v1/stream/hls/{session}/master.m3u8", get(hls_master))
        .route("/api/v1/stream/hls/{session}/{rendition}/{file}", get(hls_rendition_file))
        .route("/api/v1/stream/subtitles/{partId}/{target}", get(subtitle))
}

#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DirectStreamQuery {
    /// Short-lived HMAC-signed stream token (6 h) for players that cannot set headers.
    pub token: Option<String>,
}

/// Stream a media part directly with HTTP `Range` (`206` / `416`) and `HEAD` support.
#[utoipa::path(
    get,
    path = "/api/v1/stream/direct/{partId}",
    tag = "stream",
    params(("partId" = i64, Path), DirectStreamQuery),
    responses(
        (status = 200, description = "Full media stream"),
        (status = 206, description = "Partial byte-range stream"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Path outside library roots"),
        (status = 404, description = "Part not found"),
        (status = 416, description = "Range not satisfiable"),
    )
)]
pub async fn direct(
    State(state): State<AppState>,
    method: Method,
    Path(part_id): Path<PartId>,
    Query(query): Query<DirectStreamQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let actor = authenticate_stream(&state, &headers, query.token.as_deref(), part_id).await?;
    let resolved = state.services.stream.resolve_part(&actor, part_id).await?;
    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let is_head = method == Method::HEAD;
    Ok(mbl_stream::serve_direct_file(
        &resolved.path,
        resolved.container.as_deref(),
        range_header,
        is_head,
    )
    .await?)
}

/// Serve the upfront HLS master playlist (`master.m3u8`) for an active transcode/remux session.
#[utoipa::path(
    get,
    path = "/api/v1/stream/hls/{session}/master.m3u8",
    tag = "stream",
    params(("session" = String, Path)),
    responses(
        (status = 200, description = "HLS master playlist", content_type = "application/vnd.apple.mpegurl"),
        (status = 404, description = "Session not found"),
    )
)]
pub async fn hls_master(
    State(state): State<AppState>,
    Path(session): Path<String>,
) -> Result<Response, ApiError> {
    let m3u8 = state.services.stream.hls_master_playlist(&session).await?;
    Ok(serve_playlist(m3u8))
}

/// Serve a rendition's media playlist (`index.m3u8`), fMP4 initialization segment (`init.mp4`),
/// or numbered media segment (`{n}.m4s`).
#[utoipa::path(
    get,
    path = "/api/v1/stream/hls/{session}/{rendition}/{file}",
    tag = "stream",
    params(
        ("session" = String, Path),
        ("rendition" = String, Path),
        ("file" = String, Path, description = "`index.m3u8`, `init.mp4`, or `{n}.m4s`"),
    ),
    responses(
        (status = 200, description = "HLS media playlist or fMP4 segment"),
        (status = 404, description = "Session or rendition not found"),
        (status = 422, description = "Invalid segment filename"),
        (status = 503, description = "Transcoder unavailable or timed out"),
    )
)]
pub async fn hls_rendition_file(
    State(state): State<AppState>,
    Path((session, rendition, file)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    match file.as_str() {
        "index.m3u8" => {
            let m3u8 = state.services.stream.hls_media_playlist(&session, &rendition).await?;
            Ok(serve_playlist(m3u8))
        }
        "init.mp4" => {
            let path = state.services.stream.hls_init_segment(&session, &rendition).await?;
            Ok(serve_segment_file(&path, MIME_MP4).await?)
        }
        other => {
            let seg_num = parse_segment_filename(other)?;
            let path =
                state.services.stream.hls_media_segment(&session, &rendition, seg_num).await?;
            Ok(serve_segment_file(&path, MIME_M4S).await?)
        }
    }
}

/// Convert an embedded or external subtitle stream to WebVTT (`{streamIndex}.vtt`) or serve raw
/// ASS (`{streamIndex}.ass`), caching converted files on disk.
#[utoipa::path(
    get,
    path = "/api/v1/stream/subtitles/{partId}/{target}",
    tag = "stream",
    params(
        ("partId" = i64, Path),
        ("target" = String, Path, description = "`{streamIndex}.vtt` or `{streamIndex}.ass`"),
        DirectStreamQuery,
    ),
    responses(
        (status = 200, description = "Converted WebVTT or raw ASS subtitle track"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Path outside library roots"),
        (status = 404, description = "Part or subtitle stream not found"),
        (status = 422, description = "Unsupported subtitle target or image subtitle"),
        (status = 503, description = "FFmpeg conversion failed"),
    )
)]
pub async fn subtitle(
    State(state): State<AppState>,
    Path((part_id, target)): Path<(PartId, String)>,
    Query(query): Query<DirectStreamQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let actor = authenticate_stream(&state, &headers, query.token.as_deref(), part_id).await?;
    let (stream_index, format) = parse_subtitle_target(&target)?;
    let path =
        state.services.stream.resolve_subtitle(&actor, part_id, stream_index, format).await?;
    Ok(serve_subtitle_file(&path, format).await?)
}

async fn authenticate_stream(
    state: &AppState,
    headers: &HeaderMap,
    query_token: Option<&str>,
    part_id: PartId,
) -> mbl_core::Result<Actor> {
    if let Some(tok) = bearer_token(headers) {
        return Ok(state.services.auth.authenticate(&tok).await?.actor);
    }
    if let Some(tok) = cookie(headers, SESSION_COOKIE) {
        return Ok(state.services.auth.authenticate(&tok).await?.actor);
    }
    if let Some(tok) = query_token.map(str::trim).filter(|t| !t.is_empty()) {
        return state.services.auth.authenticate_stream_token(tok, part_id).await;
    }
    Err(Error::Unauthorized)
}
