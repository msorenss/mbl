//! Direct-play HTTP file streaming with single-range (`Range: bytes=...`) and `HEAD` support.

use std::io::SeekFrom;
use std::path::Path;

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use mbl_core::{Error, Result};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

/// Inclusive byte range `[start, end]` within a file of `total_size` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn len(self) -> u64 {
        self.end - self.start + 1
    }

    pub fn is_empty(self) -> bool {
        false
    }
}

/// Returned when a `Range` header is syntactically invalid, multi-range, or outside the file size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeUnsatisfiable {
    pub total_size: u64,
}

/// Parse an HTTP `Range` header value (`bytes=start-end`, `bytes=start-`, `bytes=-suffix`)
/// against a file of `total_size` bytes. Multi-range requests are rejected as unsatisfiable.
pub fn parse_range_header(
    raw: &str,
    total_size: u64,
) -> std::result::Result<ByteRange, RangeUnsatisfiable> {
    let err = Err(RangeUnsatisfiable { total_size });
    if total_size == 0 {
        return err;
    }
    let Some(spec) = raw.trim().strip_prefix("bytes=") else {
        return err;
    };
    let spec = spec.trim();
    if spec.is_empty() || spec.contains(',') {
        return err;
    }
    let Some((start_str, end_str)) = spec.split_once('-') else {
        return err;
    };
    let (start_str, end_str) = (start_str.trim(), end_str.trim());

    if start_str.is_empty() {
        // Suffix range: `bytes=-N` (last N bytes)
        let Ok(suffix) = end_str.parse::<u64>() else {
            return err;
        };
        if suffix == 0 {
            return err;
        }
        let start = total_size.saturating_sub(suffix);
        let end = total_size - 1;
        Ok(ByteRange { start, end })
    } else if end_str.is_empty() {
        // Open-ended range: `bytes=N-`
        let Ok(start) = start_str.parse::<u64>() else {
            return err;
        };
        if start >= total_size {
            return err;
        }
        Ok(ByteRange { start, end: total_size - 1 })
    } else {
        // Explicit range: `bytes=start-end`
        let (Ok(start), Ok(end)) = (start_str.parse::<u64>(), end_str.parse::<u64>()) else {
            return err;
        };
        if start > end || start >= total_size {
            return err;
        }
        Ok(ByteRange { start, end: end.min(total_size - 1) })
    }
}

/// Determine the MIME `Content-Type` for a media file from its extension or probed container.
pub fn content_type_for_path(path: &Path, container: Option<&str>) -> &'static str {
    let ext =
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "m4a" => "audio/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "ts" | "m2ts" | "mts" => "video/mp2t",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "mpg" | "mpeg" | "vob" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        _ => match container.unwrap_or_default() {
            c if c.contains("mp4") => "video/mp4",
            c if c.contains("webm") && !c.contains("matroska") => "video/webm",
            c if c.contains("matroska") => "video/x-matroska",
            c if c.contains("mpegts") => "video/mp2t",
            "mp3" => "audio/mpeg",
            "flac" => "audio/flac",
            "ogg" => "audio/ogg",
            "wav" => "audio/wav",
            _ => "application/octet-stream",
        },
    }
}

/// Stream a verified file path with `Range` (`206 Partial Content` / `416 Range Not Satisfiable`)
/// and `HEAD` support.
pub async fn serve_direct_file(
    path: &Path,
    container: Option<&str>,
    range_header: Option<&str>,
    is_head: bool,
) -> Result<Response> {
    let mut file = File::open(path).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::NotFound,
        _ => Error::from(e),
    })?;
    let meta = file.metadata().await.map_err(Error::from)?;
    if !meta.is_file() {
        return Err(Error::NotFound);
    }
    let total_size = meta.len();
    let content_type = content_type_for_path(path, container);

    let Some(raw_range) = range_header else {
        let body = if is_head { Body::empty() } else { Body::from_stream(ReaderStream::new(file)) };
        let mut res = (StatusCode::OK, body).into_response();
        let headers = res.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(total_size));
        return Ok(res);
    };

    let range = match parse_range_header(raw_range, total_size) {
        Ok(r) => r,
        Err(unsat) => {
            let mut res = (StatusCode::RANGE_NOT_SATISFIABLE, Body::empty()).into_response();
            let headers = res.headers_mut();
            headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            if let Ok(cr) = HeaderValue::from_str(&format!("bytes */{}", unsat.total_size)) {
                headers.insert(header::CONTENT_RANGE, cr);
            }
            return Ok(res);
        }
    };

    let length = range.len();
    let body = if is_head {
        Body::empty()
    } else {
        file.seek(SeekFrom::Start(range.start)).await.map_err(Error::from)?;
        Body::from_stream(ReaderStream::new(file.take(length)))
    };

    let mut res = (StatusCode::PARTIAL_CONTENT, body).into_response();
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    if let Ok(cr) =
        HeaderValue::from_str(&format!("bytes {}-{}/{}", range.start, range.end, total_size))
    {
        headers.insert(header::CONTENT_RANGE, cr);
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_valid_and_invalid_ranges() {
        // Start range
        assert_eq!(
            parse_range_header("bytes=0-499", 1000).unwrap(),
            ByteRange { start: 0, end: 499 }
        );
        // Middle range
        assert_eq!(
            parse_range_header("bytes=200-599", 1000).unwrap(),
            ByteRange { start: 200, end: 599 }
        );
        // Open-ended range
        assert_eq!(
            parse_range_header("bytes=600-", 1000).unwrap(),
            ByteRange { start: 600, end: 999 }
        );
        // End clamped to total_size - 1
        assert_eq!(
            parse_range_header("bytes=800-5000", 1000).unwrap(),
            ByteRange { start: 800, end: 999 }
        );
        // Suffix range
        assert_eq!(
            parse_range_header("bytes=-250", 1000).unwrap(),
            ByteRange { start: 750, end: 999 }
        );
        // Suffix larger than file clamps to whole file
        assert_eq!(
            parse_range_header("bytes=-5000", 1000).unwrap(),
            ByteRange { start: 0, end: 999 }
        );

        // Invalid cases -> RangeUnsatisfiable
        assert!(parse_range_header("items=0-5", 1000).is_err());
        assert!(parse_range_header("bytes=", 1000).is_err());
        assert!(parse_range_header("bytes=-", 1000).is_err());
        assert!(parse_range_header("bytes=-0", 1000).is_err());
        assert!(parse_range_header("bytes=500-200", 1000).is_err());
        assert!(parse_range_header("bytes=1000-1000", 1000).is_err());
        assert!(parse_range_header("bytes=1000-", 1000).is_err());
        assert!(parse_range_header("bytes=0-10, 20-30", 1000).is_err());
        assert!(parse_range_header("bytes=0-0", 0).is_err());
    }

    #[test]
    fn mime_detection() {
        assert_eq!(content_type_for_path(Path::new("/a/b.mp4"), None), "video/mp4");
        assert_eq!(content_type_for_path(Path::new("/a/b.mkv"), None), "video/x-matroska");
        assert_eq!(content_type_for_path(Path::new("/a/b.webm"), None), "video/webm");
        assert_eq!(content_type_for_path(Path::new("/a/b.flac"), None), "audio/flac");
        assert_eq!(content_type_for_path(Path::new("/a/b.mp3"), None), "audio/mpeg");
    }
}
