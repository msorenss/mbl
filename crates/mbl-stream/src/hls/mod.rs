//! HLS playlists and segment response helpers (`master.m3u8`, `index.m3u8`, `init.mp4`, `{n}.m4s`).

pub mod playlist;

use std::path::Path;

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use mbl_core::{Error, Result};

pub use playlist::{
    HlsRendition, render_master_playlist, render_media_playlist, segment_durations_ms,
    segment_start_ms,
};

pub const MIME_M3U8: &str = "application/vnd.apple.mpegurl";
pub const MIME_MP4: &str = "video/mp4";
pub const MIME_M4S: &str = "video/iso.segment";

/// Build an HTTP response for an `.m3u8` playlist string.
pub fn serve_playlist(content: String) -> Response {
    let mut res = Response::new(Body::from(content));
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(MIME_M3U8));
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    res
}

/// Read an on-disk `init.mp4` or `{n}.m4s` segment and build an HTTP response with `content_type`.
pub async fn serve_segment_file(path: &Path, content_type: &'static str) -> Result<Response> {
    let bytes = tokio::fs::read(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::NotFound
        } else {
            Error::Internal(Box::new(e))
        }
    })?;

    let mut res = Response::new(Body::from(bytes));
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

/// Parse `{n}.m4s` into its `u32` segment index.
pub fn parse_segment_filename(filename: &str) -> Result<u32> {
    let stem = filename
        .strip_suffix(".m4s")
        .ok_or_else(|| Error::Validation(format!("invalid segment filename '{filename}'")))?;
    stem.parse::<u32>()
        .map_err(|_| Error::Validation(format!("invalid segment number in '{filename}'")))
}
