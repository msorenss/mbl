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
    // Wait for a batch extraction of this part that may be writing this very file.
    let lock = part_lock(part_id);
    let _guard = lock.lock().await;

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

/// Path of the cached WebVTT file for one stream of a part.
pub fn cached_vtt_path(cache_dir: &Path, part_id: PartId, stream_index: u32) -> PathBuf {
    cache_dir.join("subtitles").join(format!("{}_{}.vtt", part_id.0, stream_index))
}

/// One batch extraction per part at a time; a second caller waits and then finds the files.
fn part_lock(part_id: PartId) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    static LOCKS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<i64, std::sync::Weak<tokio::sync::Mutex<()>>>>,
    > = std::sync::OnceLock::new();
    let mut map = LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = map.get(&part_id.0).and_then(std::sync::Weak::upgrade) {
        return lock;
    }
    let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
    map.insert(part_id.0, std::sync::Arc::downgrade(&lock));
    lock
}

/// Receives decoded audio in chunks (see [`extract_embedded_vtt`]).
pub type AudioSink<'a> = dyn FnMut(&[u8]) + Send + 'a;

/// Extract the embedded text subtitle streams `indices` of `media_path` to WebVTT in the cache,
/// all in one FFmpeg run. Subtitles are spread over the whole film, so FFmpeg reads the entire
/// file for any one of them: 4.9 GB from an SD card took over two minutes per track. One run
/// writes every track for the price of one. Tracks already cached are skipped; returns how many
/// were written. Fails as a whole when FFmpeg fails (then extract the track on its own).
///
/// With `audio`, the same run also decodes the first audio track to 8 kHz mono 16-bit PCM in the
/// voice band and hands it over in chunks, for finding where people speak (subtitle sync)
/// without reading the file a second time. FFmpeg then runs even when every track is cached.
pub async fn extract_embedded_vtt(
    cache_dir: &Path,
    ffmpeg_bin: &Path,
    part_id: PartId,
    media_path: &Path,
    indices: &[u32],
    mut audio: Option<&mut AudioSink<'_>>,
) -> Result<usize> {
    use tokio::io::AsyncReadExt;
    let lock = part_lock(part_id);
    let _guard = lock.lock().await;
    let mut missing = Vec::new();
    for &index in indices {
        let cached = tokio::fs::metadata(cached_vtt_path(cache_dir, part_id, index)).await;
        if !cached.is_ok_and(|m| m.is_file() && m.len() > 0) {
            missing.push(index);
        }
    }
    if missing.is_empty() && audio.is_none() {
        return Ok(0);
    }
    let sub_cache_dir = cache_dir.join("subtitles");
    tokio::fs::create_dir_all(&sub_cache_dir).await.map_err(Error::internal)?;
    let unique = SUBTITLE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let outputs: Vec<(u32, PathBuf)> = missing
        .iter()
        .map(|&i| (i, sub_cache_dir.join(format!("{}_{}.{}.batch.tmp", part_id.0, i, unique))))
        .collect();

    let mut command = Command::new(ffmpeg_bin);
    command.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(media_path);
    for (index, tmp) in &outputs {
        command
            .arg("-map")
            .arg(format!("0:{index}"))
            .args(["-c:s", "webvtt", "-f", "webvtt"])
            .arg(tmp);
    }
    if audio.is_some() {
        command.args([
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "8000",
            "-af",
            "highpass=f=200,lowpass=f=3500",
            "-f",
            "s16le",
            "pipe:1",
        ]);
    }
    let cleanup = |outputs: &[(u32, PathBuf)]| {
        for (_, tmp) in outputs {
            let _ = std::fs::remove_file(tmp);
        }
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(if audio.is_some() { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            Error::Unavailable(format!("failed to launch ffmpeg (`{}`): {e}", ffmpeg_bin.display()))
        })?;
    if let (Some(feed), Some(mut stdout)) = (audio.as_mut(), child.stdout.take()) {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match stdout.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => feed(&buffer[..n]),
                Err(error) => {
                    cleanup(&outputs);
                    return Err(Error::internal(error));
                }
            }
        }
    }
    let output = child.wait_with_output().await.map_err(Error::internal)?;
    if !output.status.success() {
        cleanup(&outputs);
        return Err(Error::Unavailable(format!(
            "ffmpeg subtitle extraction failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let mut written = 0;
    for (index, tmp) in outputs {
        tokio::fs::rename(&tmp, cached_vtt_path(cache_dir, part_id, index))
            .await
            .map_err(Error::internal)?;
        written += 1;
    }
    Ok(written)
}

/// Milliseconds from a WebVTT timestamp (`hh:mm:ss.mmm` or `mm:ss.mmm`).
fn vtt_time(text: &str) -> Option<i64> {
    let (clock, millis) = text.trim().split_once('.')?;
    let parts: Vec<i64> = clock.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let seconds = match parts.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    };
    Some(seconds * 1000 + millis.get(..3)?.parse::<i64>().ok()?)
}

fn format_vtt_time(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// The (start, end) of every cue in a WebVTT file, in milliseconds.
pub fn vtt_cues(vtt: &str) -> Vec<(i64, i64)> {
    vtt.lines()
        .filter_map(|line| {
            let (start, rest) = line.split_once("-->")?;
            let end = rest.split_whitespace().next()?;
            Some((vtt_time(start)?, vtt_time(end)?))
        })
        .collect()
}

/// The WebVTT file with every cue's times passed through `shift`; cue settings after the end
/// time and everything else are kept as they are.
pub fn shift_vtt(vtt: &str, shift: impl Fn(i64) -> i64) -> String {
    let mut out = String::with_capacity(vtt.len());
    for line in vtt.lines() {
        let shifted = line.split_once("-->").and_then(|(start, rest)| {
            let rest = rest.trim_start();
            let (end, settings) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let (start, end) = (vtt_time(start)?, vtt_time(end)?);
            let mut line =
                format!("{} --> {}", format_vtt_time(shift(start)), format_vtt_time(shift(end)));
            if !settings.trim().is_empty() {
                line.push(' ');
                line.push_str(settings.trim());
            }
            Some(line)
        });
        out.push_str(shifted.as_deref().unwrap_or(line));
        out.push('\n');
    }
    out
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
    fn webvtt_cues_are_read_and_shifted() {
        let vtt = "WEBVTT\n\n00:01.500 --> 00:03.000 align:start\nHello\n\n01:00:02.250 --> 01:00:04.000\nBye\n";
        assert_eq!(vtt_cues(vtt), vec![(1_500, 3_000), (3_602_250, 3_604_000)]);
        let shifted = shift_vtt(vtt, |ms| ms - 2_000);
        assert_eq!(
            shifted,
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.000 align:start\nHello\n\n01:00:00.250 --> 01:00:02.000\nBye\n"
        );
    }

    #[test]
    fn parses_subtitle_targets() {
        assert_eq!(parse_subtitle_target("2.vtt").unwrap(), (2, SubtitleFormat::Vtt));
        assert_eq!(parse_subtitle_target("5.ass").unwrap(), (5, SubtitleFormat::Ass));
        assert!(parse_subtitle_target("2.srt").is_err());
        assert!(parse_subtitle_target("abc.vtt").is_err());
        assert!(parse_subtitle_target("novtt").is_err());
    }
}
