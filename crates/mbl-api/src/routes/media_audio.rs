//! Progressive audio transcode streaming endpoint (M7.2).

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::StreamExt;
use mbl_analysis::{AudioCodec, build_audio_transcode_args};
use mbl_core::PartId;
use serde::Deserialize;
use std::process::Stdio;
use tokio::process::{Child, Command};
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
        None | Some("opus") => AudioCodec::Opus,
        _ => return Err(mbl_core::Error::Validation("unsupported audio codec".into()).into()),
    };
    let bitrate = q.bitrate.unwrap_or(160);
    let seek = q.seek;
    if !(32..=512).contains(&bitrate) || seek.is_some_and(|n| !n.is_finite() || n < 0.0) {
        return Err(
            mbl_core::Error::Validation("invalid audio bitrate or seek position".into()).into()
        );
    }

    let args = build_audio_transcode_args(&resolved.path, codec, bitrate, seek);

    let ffmpeg_path = state.services.stream.ffmpeg_bin();
    let child = Command::new(ffmpeg_path)
        .args(&args)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| mbl_core::Error::External(format!("failed to spawn ffmpeg: {e}")))?;

    let body = child_audio_body(child)?;

    let mut headers = HeaderMap::new();
    if let Ok(mime) = codec.mime_type().parse() {
        headers.insert(header::CONTENT_TYPE, mime);
    }
    headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-cache"));

    Ok((headers, body).into_response())
}

fn child_audio_body(mut child: Child) -> Result<axum::body::Body, ApiError> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| mbl_core::Error::External("missing ffmpeg stdout".into()))?;
    // The response owns the child until EOF or client disconnect, when kill_on_drop
    // terminates it. EOF explicitly waits for reaping before releasing the handle.
    let stream = futures_util::stream::unfold(
        (ReaderStream::new(stdout), child),
        |(mut output, mut child)| async move {
            match output.next().await {
                Some(chunk) => Some((chunk, (output, child))),
                None => {
                    let _ = child.wait().await;
                    None
                }
            }
        },
    );
    Ok(axum::body::Body::from_stream(stream))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn disconnect_terminates_the_owned_transcoder() {
        let child = Command::new("/bin/sh")
            .args(["-c", "printf audio; exec sleep 60"])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let mut body = child_audio_body(child).unwrap().into_data_stream();
        assert_eq!(&body.next().await.unwrap().unwrap()[..], b"audio");
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
        drop(body);
        for _ in 0..80 {
            if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("transcoder survived response disconnect");
    }
}
