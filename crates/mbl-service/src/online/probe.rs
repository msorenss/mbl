//! Probe only a local bounded snapshot; untrusted manifests cannot make ffprobe fetch URLs.
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use mbl_core::{Error, Result};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub(super) async fn media_sample(bytes: &[u8], tools: Option<&mbl_probe::FfTools>) -> Result<bool> {
    let tools = tools.ok_or_else(|| {
        Error::Unavailable("FFmpeg and FFprobe are required to check online media".into())
    })?;
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(bytes)?;
    file.flush()?;
    let mut input = std::ffi::OsString::from("file:");
    input.push(file.path());
    let mut child = Command::new(&tools.ffprobe)
        .args([
            "-v",
            "quiet",
            "-protocol_whitelist",
            "file",
            "-format_whitelist",
            "mov,matroska,webm,mpegts,mp3,aac,ogg,flac,wav",
            "-probesize",
            "4194304",
            "-analyzeduration",
            "2000000",
            "-max_alloc",
            "16777216",
            "-show_entries",
            "stream=codec_type,codec_name",
            "-of",
            "json",
        ])
        .arg(&input)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Error::Unavailable("ffprobe is required to check online media".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Unavailable("Media checker output is unavailable".into()))?;
    let run = async {
        let mut output = Vec::new();
        stdout.take(65537).read_to_end(&mut output).await?;
        if output.len() > 65536 {
            return Err(Error::Unavailable("Media checker output exceeded its limit".into()));
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(Error::Unavailable("The sample could not be verified as media".into()));
        }
        let doc: serde_json::Value = serde_json::from_slice(&output).map_err(Error::internal)?;
        let streams = doc
            .get("streams")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| Error::Unavailable("No media streams detected".into()))?;
        let video = streams.iter().any(|s| {
            s["codec_type"] == "video" && s["codec_name"].as_str().is_some_and(|v| v != "unknown")
        });
        let audio = streams.iter().any(|s| {
            s["codec_type"] == "audio" && s["codec_name"].as_str().is_some_and(|v| v != "unknown")
        });
        if !video && !audio {
            return Err(Error::Unavailable("No video or audio stream was verified".into()));
        }
        Ok(video)
    };
    let has_video = tokio::time::timeout(Duration::from_secs(4), run)
        .await
        .map_err(|_| Error::Unavailable("Media check timed out".into()))??;
    // An audio-frame limit can end a combined output before its first video frame.
    // Decode the selected primary stream so actual video must reach the output.
    let mut decoder = Command::new(&tools.ffmpeg)
        .args([
            "-nostdin",
            "-v",
            "error",
            "-xerror",
            "-threads",
            "1",
            "-protocol_whitelist",
            "file",
            "-format_whitelist",
            "mov,matroska,webm,mpegts,mp3,aac,ogg,flac,wav",
            "-probesize",
            "4194304",
            "-analyzeduration",
            "2000000",
            "-max_alloc",
            "67108864",
            "-i",
        ])
        .arg(&input)
        .args([
            "-map",
            if has_video { "0:v:0" } else { "0:a:0" },
            "-frames:v",
            "1",
            "-frames:a",
            "1",
            "-f",
            "null",
            "-progress",
            "pipe:1",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Error::Unavailable("ffmpeg is required to check online media".into()))?;
    let output = decoder
        .stdout
        .take()
        .ok_or_else(|| Error::Unavailable("Decoder output is unavailable".into()))?;
    let decode = async {
        let mut progress = Vec::new();
        output.take(65537).read_to_end(&mut progress).await?;
        if progress.len() > 65536 {
            return Err(Error::Unavailable("Decoder output exceeded its limit".into()));
        }
        let ok = decoder.wait().await?.success();
        let decoded = String::from_utf8_lossy(&progress)
            .lines()
            .filter_map(|l| l.split_once('='))
            .any(|(key, value)| {
                key == if has_video { "frame" } else { "out_time_us" }
                    && value.trim().parse::<u64>().is_ok_and(|v| v > 0)
            });
        if !ok || !decoded {
            return Err(Error::Unavailable(
                "The sample could not be decoded. Playback has not been verified.".into(),
            ));
        }
        Ok(has_video)
    };
    tokio::time::timeout(Duration::from_secs(4), decode)
        .await
        .map_err(|_| Error::Unavailable("Sample decoding timed out".into()))?
}
