//! Subtitle extraction, conversion to WebVTT (`.vtt`) / raw ASS (`.ass`) via FFmpeg, and disk caching.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use mbl_core::{Error, PartId, Result};
use tokio::process::Command;

static SUBTITLE_TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Output format requested by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleFormat {
    Vtt,
    Ass,
}

impl SubtitleFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Vtt => "vtt",
            Self::Ass => "ass",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Vtt => "text/vtt; charset=utf-8",
            Self::Ass => "text/x-ssa; charset=utf-8",
        }
    }
}

/// Source of a subtitle stream on disk or inside a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubtitleSource {
    Embedded { media_path: PathBuf, stream_index: u32, codec: String },
    External { path: PathBuf, codec: String },
}

impl SubtitleSource {
    pub fn codec(&self) -> &str {
        match self {
            Self::Embedded { codec, .. } | Self::External { codec, .. } => codec,
        }
    }
}

/// Parse `{streamIndex}.vtt` or `{streamIndex}.ass` into `(stream_index, format)`.
pub fn parse_subtitle_target(target: &str) -> Result<(u32, SubtitleFormat)> {
    let (idx_str, ext) = target
        .rsplit_once('.')
        .ok_or_else(|| Error::Validation(format!("invalid subtitle target `{target}`")))?;
    let format = match ext {
        "vtt" => SubtitleFormat::Vtt,
        "ass" => SubtitleFormat::Ass,
        _ => {
            return Err(Error::Validation(format!(
                "unsupported subtitle format `.{ext}` (expected `.vtt` or `.ass`)"
            )));
        }
    };
    let stream_index = idx_str
        .parse::<u32>()
        .map_err(|_| Error::Validation(format!("invalid subtitle stream index `{idx_str}`")))?;
    Ok((stream_index, format))
}

/// Returns `true` if `codec` is a bitmap/image subtitle format (PGS, VobSub, DVB) that cannot be
/// converted to text WebVTT/ASS.
pub fn is_image_subtitle(codec: &str) -> bool {
    matches!(
        codec.trim().to_ascii_lowercase().as_str(),
        "hdmv_pgs_subtitle" | "pgs" | "pgssub" | "dvd_subtitle" | "vobsub" | "dvb_subtitle"
    )
}

/// Convert or extract a subtitle stream into `<cache_dir>/subtitles/{part_id}_{stream_index}.{ext}`,
/// returning the path on disk. Cached files are reused without re-invoking FFmpeg.
pub async fn extract_or_convert_subtitle(
    cache_dir: &Path,
    ffmpeg_bin: &Path,
    part_id: PartId,
    stream_index: u32,
    source: &SubtitleSource,
    format: SubtitleFormat,
) -> Result<PathBuf> {
    if is_image_subtitle(source.codec()) {
        return Err(Error::Validation(format!(
            "image subtitle codec `{}` cannot be converted to `.{}`",
            source.codec(),
            format.extension()
        )));
    }

    // Fast path: external raw ASS file served directly when `.ass` is requested.
    if format == SubtitleFormat::Ass
        && let SubtitleSource::External { path, codec } = source
        && matches!(codec.to_ascii_lowercase().as_str(), "ass" | "ssa")
    {
        return Ok(path.clone());
    }

    let sub_cache_dir = cache_dir.join("subtitles");
    let cached_path =
        sub_cache_dir.join(format!("{}_{}.{}", part_id.0, stream_index, format.extension()));

    if let Ok(meta) = tokio::fs::metadata(&cached_path).await
        && meta.is_file()
        && meta.len() > 0
    {
        return Ok(cached_path);
    }

    tokio::fs::create_dir_all(&sub_cache_dir).await.map_err(Error::internal)?;

    let unique = SUBTITLE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = sub_cache_dir.join(format!(
        "{}_{}.{}.{}.tmp",
        part_id.0,
        stream_index,
        unique,
        format.extension()
    ));

    let (input_path, map_spec) = match source {
        SubtitleSource::Embedded { media_path, stream_index, .. } => {
            (media_path.as_path(), format!("0:{stream_index}"))
        }
        SubtitleSource::External { path, .. } => (path.as_path(), "0:0".to_owned()),
    };

    let (codec_arg, muxer_arg) = match format {
        SubtitleFormat::Vtt => ("webvtt", "webvtt"),
        SubtitleFormat::Ass => {
            if matches!(source.codec().to_ascii_lowercase().as_str(), "ass" | "ssa") {
                ("copy", "ass")
            } else {
                ("ass", "ass")
            }
        }
    };

    let output = Command::new(ffmpeg_bin)
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input_path)
        .arg("-map")
        .arg(&map_spec)
        .arg("-c:s")
        .arg(codec_arg)
        .arg("-f")
        .arg(muxer_arg)
        .arg(&tmp_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            Error::Unavailable(format!("failed to launch ffmpeg (`{}`): {e}", ffmpeg_bin.display()))
        })?;

    if !output.status.success() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Unavailable(format!(
            "ffmpeg subtitle conversion failed: {}",
            stderr.trim()
        )));
    }

    tokio::fs::rename(&tmp_path, &cached_path).await.map_err(Error::internal)?;

    Ok(cached_path)
}

/// Serve a converted or raw subtitle file with the appropriate MIME type and caching headers.
pub async fn serve_subtitle_file(path: &Path, format: SubtitleFormat) -> Result<Response> {
    let bytes = tokio::fs::read(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound { Error::NotFound } else { Error::internal(e) }
    })?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, format.content_type())
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .map_err(Error::internal)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_subtitle_targets() {
        assert_eq!(parse_subtitle_target("2.vtt").unwrap(), (2, SubtitleFormat::Vtt));
        assert_eq!(parse_subtitle_target("5.ass").unwrap(), (5, SubtitleFormat::Ass));
        assert!(parse_subtitle_target("2.srt").is_err());
        assert!(parse_subtitle_target("abc.vtt").is_err());
        assert!(parse_subtitle_target("novtt").is_err());
    }
}
