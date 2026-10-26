//! Progressive audio transcode streaming endpoint (M7.2).

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use mbl_analysis::{AudioCodec, build_audio_transcode_args};
use mbl_core::PartId;
use serde::Deserialize;
use std::process::Stdio;
use tokio::process::Command;
use tokio_util::io::ReaderStream;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct AudioStreamQuery {
    pub codec: Option<String>,
    pub bitrate: Option<u32>,
    pub seek: Option<f64>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/stream/audio/{partId}", get(stream_audio))
}

#[utoipa::path(
    get,
    path = "/api/v1/stream/audio/{partId}",
    params(
        ("partId" = i64, Path, description = "Media Part ID"),
        ("codec" = Option<String>, Query, description = "Target audio codec: opus, aac, mp3 (default: opus)"),
        ("bitrate" = Option<u32>, Query, description = "Target bitrate in kbps (default: 160)"),
        ("seek" = Option<f64>, Query, description = "Seek offset in seconds (default: 0)")
    ),
    responses(
        (status = 200, description = "Progressive audio stream", content_type = "audio/ogg"),
    ),
    security(("bearer_auth" = []))
)]
async fn stream_audio(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(part_id): Path<i64>,
    Query(q): Query<AudioStreamQuery>,
) -> Result<Response, ApiError> {
    let resolved = state.services.stream.resolve_part(&auth.actor, PartId(part_id)).await?;

    let codec = match q.codec.as_deref() {
        Some("aac") => AudioCodec::Aac,
        Some("mp3") => AudioCodec::Mp3,
        _ => AudioCodec::Opus,
    };
    let bitrate = q.bitrate.unwrap_or(160);
    let seek = q.seek;

    let args = build_audio_transcode_args(&resolved.path, codec, bitrate, seek);

    let ffmpeg_path = "ffmpeg";
    let mut child = Command::new(ffmpeg_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| mbl_core::Error::External(format!("failed to spawn ffmpeg: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| mbl_core::Error::External("missing ffmpeg stdout".into()))?;

    let stream = ReaderStream::new(stdout);
    let body = axum::body::Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    if let Ok(mime) = codec.mime_type().parse() {
        headers.insert(header::CONTENT_TYPE, mime);
    }
    headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-cache"));

    Ok((headers, body).into_response())
}
