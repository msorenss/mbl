//! Locating and running `ffmpeg` / `ffprobe` as child processes (ADR 0001).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use mbl_core::{Error, Result};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::model::MediaInfo;
use crate::parse::parse_ffprobe_json;

/// Probe output larger than this is treated as hostile.
const MAX_STDOUT: u64 = 16 * 1024 * 1024;
const MAX_STDERR: u64 = 8 * 1024;
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct FfTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl FfTools {
    /// Find working binaries: configured path, then next to our executable, then `PATH`.
    pub async fn locate(ffmpeg: &str, ffprobe: &str) -> Result<Self> {
        Ok(Self {
            ffmpeg: find_binary("ffmpeg", ffmpeg).await?,
            ffprobe: find_binary("ffprobe", ffprobe).await?,
        })
    }

    /// First line of `ffmpeg -version`.
    pub async fn version(&self) -> Result<String> {
        let out = Command::new(&self.ffmpeg).arg("-version").output().await?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().to_owned())
    }

    /// Probe a file. Fails with `Validation` for unreadable media and `Unavailable` on timeout.
    pub async fn probe(&self, path: &Path, timeout: Duration) -> Result<MediaInfo> {
        // The `file:` prefix stops FFmpeg from interpreting names as protocols ("concat:", "http:").
        let mut input = std::ffi::OsString::from("file:");
        input.push(path.as_os_str());

        let mut child = Command::new(&self.ffprobe)
            .args(["-v", "error", "-show_format", "-show_streams", "-show_chapters", "-of", "json"])
            .arg(input)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let run = async {
            let (out, err) =
                tokio::join!(read_limited(stdout, MAX_STDOUT), read_limited(stderr, MAX_STDERR));
            let status = child.wait().await?;
            Ok::<_, Error>((status, out?, err?))
        };
        let (status, out, err) = tokio::time::timeout(timeout, run)
            .await
            .map_err(|_| Error::Unavailable(format!("ffprobe timed out after {timeout:?}")))??;

        if out.len() as u64 > MAX_STDOUT {
            return Err(Error::Validation("ffprobe output too large".into()));
        }
        if !status.success() {
            let msg = String::from_utf8_lossy(&err);
            return Err(Error::Validation(format!("not a readable media file: {}", msg.trim())));
        }
        parse_ffprobe_json(&out)
    }
}

async fn read_limited(reader: Option<impl AsyncRead + Unpin>, limit: u64) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    if let Some(r) = reader {
        r.take(limit + 1).read_to_end(&mut buf).await?;
    }
    Ok(buf)
}

async fn find_binary(name: &str, configured: &str) -> Result<PathBuf> {
    let exe_name = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    let mut candidates = Vec::new();
    if !configured.trim().is_empty() {
        candidates.push(PathBuf::from(configured.trim()));
    } else {
        if let Some(dir) =
            std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
        {
            candidates.push(dir.join(&exe_name));
        }
        candidates.push(PathBuf::from(&exe_name));
    }
    for candidate in candidates {
        let ok = Command::new(&candidate)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|s| s.success());
        if ok {
            return Ok(candidate);
        }
    }
    Err(Error::Unavailable(format!(
        "{name} not found; install FFmpeg >= 6.1 or set transcode.{name}_path"
    )))
}
