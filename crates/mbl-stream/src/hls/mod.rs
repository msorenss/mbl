//! HLS playlists and segment response helpers (`master.m3u8`, `index.m3u8`, `init.mp4`, `{n}.m4s`).

pub mod playlist;

use std::path::Path;

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use mbl_core::{Error, Result};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

pub use playlist::{
    HlsRendition, render_master_playlist, render_media_playlist, render_media_playlist_with_target,
    segment_durations_ms, segment_start_ms, segment_start_ms_full,
};

pub const MIME_M3U8: &str = "application/vnd.apple.mpegurl";
pub const MIME_MP4: &str = "video/mp4";
pub const MIME_M4S: &str = "video/iso.segment";

/// Build an HTTP response for an `.m3u8` playlist string.
pub fn serve_playlist(content: String) -> Response {
    let len = content.len();
    let mut res = Response::new(Body::from(content));
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(MIME_M3U8));
    res.headers_mut().insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    res
}

/// Stream an on-disk `init.mp4` or `{n}.m4s` segment with the given `content_type`.
pub async fn serve_segment_file(path: &Path, content_type: &'static str) -> Result<Response> {
    let file = File::open(path).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::NotFound,
        _ => Error::from(e),
    })?;
    let metadata = file.metadata().await.map_err(Error::from)?;
    if !metadata.is_file() {
        return Err(Error::NotFound);
    }

    let len = metadata.len();
    let body = Body::from_stream(ReaderStream::with_capacity(file.take(len), 128 * 1024));
    let mut res = Response::new(body);
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    res.headers_mut().insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    res.headers_mut().insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    // A seek or encoder fallback can regenerate files under the same session URLs.
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
