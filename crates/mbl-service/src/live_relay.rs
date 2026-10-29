//! Owned live HLS encoder. Playlists are produced by FFmpeg, never fabricated up front.
use mbl_core::{Error, Result};
use mbl_transcode::VideoCapacity;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug)]
struct State {
    child: Option<Child>,
    closed: bool,
    capacity: Option<VideoCapacity>,
}
#[derive(Debug)]
pub(crate) struct LiveRelay {
    state: Mutex<State>,
    directory: tempfile::TempDir,
}
impl LiveRelay {
    pub(crate) fn spawn(
        ffmpeg: &Path,
        root: &Path,
        source: &str,
        capacity: VideoCapacity,
    ) -> Result<Arc<Self>> {
        std::fs::create_dir_all(root).map_err(Error::internal)?;
        let directory =
            tempfile::Builder::new().prefix("live-").tempdir_in(root).map_err(Error::internal)?;
        let mut command = Command::new(ffmpeg);
        command.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y",
            "-protocol_whitelist", "http,https,tcp,tls,crypto", "-rw_timeout", "10000000", "-i", source,
            "-map", "0:v:0?", "-map", "0:a:0?",
            "-vf", "bwdif=mode=send_frame:parity=auto:deint=interlaced,scale=w='min(1280,iw)':h='min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2,format=yuv420p",
            "-r", "25", "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency",
            "-b:v", "2500k", "-maxrate", "3000k", "-bufsize", "6000k", "-g", "50", "-keyint_min", "50", "-sc_threshold", "0", "-threads", "2",
            "-c:a", "aac", "-ac", "2", "-ar", "48000", "-b:a", "128k",
            "-f", "hls", "-hls_time", "2", "-hls_list_size", "6", "-hls_delete_threshold", "2",
            "-hls_flags", "delete_segments+independent_segments+temp_file", "-hls_segment_type", "fmp4",
            "-hls_fmp4_init_filename", "init.mp4", "-hls_segment_filename"])
            .arg(directory.path().join("seg_%09d.m4s"))
            .arg(directory.path().join("index.m3u8"))
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
        let child = command.spawn().map_err(|error| {
            Error::Unavailable(format!("could not start live encoder: {error}"))
        })?;
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                child: Some(child),
                closed: false,
                capacity: Some(capacity),
            }),
            directory,
        }))
    }
    pub(crate) async fn failed(&self) -> bool {
        let mut state = self.state.lock().await;
        state.closed
            || state.child.as_mut().is_some_and(|child| match child.try_wait() {
                Ok(Some(status)) => !status.success(),
                Ok(None) => false,
                Err(_) => true,
            })
    }
    pub(crate) async fn prepare(&self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if self.failed().await {
                    return Err(Error::Unavailable(
                        "live source or encoder failed before playback was ready".into(),
                    ));
                }
                if let Ok(playlist) =
                    tokio::fs::read_to_string(self.directory.path().join("index.m3u8")).await
                {
                    let segment = playlist
                        .lines()
                        .find(|line| line.starts_with("seg_") && valid_filename(line));
                    if playlist.contains("#EXTINF:")
                        && let Some(segment) = segment
                        && nonempty(&self.directory.path().join("init.mp4")).await
                        && nonempty(&self.directory.path().join(segment)).await
                    {
                        return Ok(());
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .map_err(|_| Error::Unavailable("timed out preparing live HLS".into()))?
    }
    pub(crate) async fn file(&self, filename: &str) -> Result<PathBuf> {
        if !valid_filename(filename) || self.failed().await {
            return Err(Error::NotFound);
        }
        Ok(self.directory.path().join(filename))
    }
    pub(crate) async fn shutdown(&self) {
        let mut state = self.state.lock().await;
        state.closed = true;
        if let Some(mut child) = state.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        state.capacity = None;
        for attempt in 0..5 {
            match tokio::fs::remove_dir_all(self.directory.path()).await {
                Ok(()) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(_) if attempt < 4 => tokio::time::sleep(Duration::from_millis(100)).await,
                Err(error) => {
                    tracing::warn!(%error, "could not remove live HLS directory");
                    break;
                }
            }
        }
    }
}
async fn nonempty(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok_and(|m| m.is_file() && m.len() > 0)
}
fn valid_filename(name: &str) -> bool {
    matches!(name, "index.m3u8" | "init.mp4")
        || name.len() <= 40
            && name
                .strip_prefix("seg_")
                .and_then(|s| s.strip_suffix(".m4s"))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}
