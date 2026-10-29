//! Direct-play and HLS streaming endpoints.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
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
        .route("/api/v1/stream/direct/{partId}", get(direct).options(media_preflight))
        .route("/api/v1/stream/hls/{session}/master.m3u8", get(hls_master).options(media_preflight))
        .route(
            "/api/v1/stream/hls/{session}/{rendition}/{file}",
            get(hls_rendition_file).options(media_preflight),
        )
        .route("/api/v1/stream/live/{session}/{file}", get(live_file).options(media_preflight))
        .route("/api/v1/stream/subtitles/{partId}/{target}", get(subtitle).options(media_preflight))
        .layer(axum::middleware::map_response(media_cors))
}

// Cast receivers fetch media from a different origin. These headers apply only
// to media routes; their existing signed-token/capability checks remain in force.
async fn media_preflight() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn media_cors(request_headers: HeaderMap, mut response: Response) -> Response {
    let headers = response.headers_mut();
    if let Some(origin) = request_headers.get(header::ORIGIN) {
        if origin
            .to_str()
            .ok()
            .is_some_and(|value| value.starts_with("https://") || value.starts_with("http://"))
        {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone());
            headers.append(header::VARY, HeaderValue::from_static("Origin"));
        }
    } else {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, HEAD, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Range, Content-Type, Accept-Encoding"),
    );
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Accept-Ranges, Content-Range, Content-Length"),
    );
    response
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

/// Read a live HLS capability URL while its authenticated owner's lease remains active.
#[utoipa::path(get, path = "/api/v1/stream/live/{session}/{file}", tag = "stream",
    params(("session" = String, Path), ("file" = String, Path)),
    responses((status = 200, description = "Live HLS playlist or fMP4 media"), (status = 404, description = "Expired lease or unavailable media")))]
pub async fn live_file(
    State(state): State<AppState>,
    method: Method,
    Path((session, file)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let path = state.services.livetv.live_file(&session, &file).await?;
    let mut response = if file == "index.m3u8" {
        let playlist = tokio::fs::read_to_string(&path).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound
            } else {
                Error::internal(error)
            }
        })?;
        let length = playlist.len();
        let mut response = serve_playlist(playlist);
        response.headers_mut().insert(header::CONTENT_LENGTH, HeaderValue::from(length));
        if method == Method::HEAD {
            *response.body_mut() = axum::body::Body::empty();
        }
        response
    } else {
        mbl_stream::serve_direct_file(
            &path,
            None,
            headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
            method == Method::HEAD,
        )
        .await?
    };
    if file.ends_with(".m4s") {
        response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(MIME_M4S));
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
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

pub async fn limit_wan(
    State(state): State<AppState>,
    crate::extract::ClientNetwork(network): crate::extract::ClientNetwork,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    use futures_util::StreamExt;
    let media =
        request.method() == Method::GET && request.uri().path().starts_with("/api/v1/stream/");
    let response = next.run(request).await;
    if !media || !network.is_wan() || !response.status().is_success() {
        return response;
    }
    let bandwidth = state.services.remote.bandwidth().clone();
    let (parts, body) = response.into_parts();
    let stream = body
        .into_data_stream()
        .flat_map(|result| {
            let chunks: Vec<Result<axum::body::Bytes, axum::Error>> = match result {
                Ok(data) => (0..data.len())
                    .step_by(65_536)
                    .map(|offset| Ok(data.slice(offset..(offset + 65_536).min(data.len()))))
                    .collect(),
                Err(error) => vec![Err(error)],
            };
            futures_util::stream::iter(chunks)
        })
        .then(move |chunk| {
            let bandwidth = bandwidth.clone();
            async move {
                if let Ok(data) = &chunk {
                    bandwidth.throttle(data.len()).await;
                }
                chunk
            }
        });
    Response::from_parts(parts, axum::body::Body::from_stream(stream))
}
