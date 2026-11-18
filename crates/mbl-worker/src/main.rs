//! MBL transcode worker CLI: `pair` once, then `run` to take encodes from the server.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mbl_core::{WorkerCapabilities, WorkerPairRequest, WorkerPairResponse};
use mbl_worker::{
    DaemonOptions, WorkerConfigFile, default_config_path, default_max_jobs, detect_encoders,
};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Parser)]
#[command(name = "mbl-worker", version, about = "MBL Transcode Worker")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pair this worker with an MBL server using a one-time pairing code
    Pair {
        /// Base URL of the central MBL server (e.g. http://192.168.1.50:8420)
        #[arg(long)]
        server: String,
        /// One-time pairing code generated on the server (e.g. W-123456)
        #[arg(long)]
        code: String,
        /// Friendly name for this worker (defaults to the host name)
        #[arg(long)]
        name: Option<String>,
        /// MAC address for Wake-on-LAN (optional, e.g. 00:1A:2B:3C:4D:5E)
        #[arg(long)]
        mac: Option<String>,
        /// Where to save the worker configuration (default: %ProgramData%\MBL\worker.json on Windows)
        #[arg(long)]
        config: Option<PathBuf>,
        /// FFmpeg binary used for the hardware self-test
        #[arg(long)]
        ffmpeg: Option<PathBuf>,
    },
    /// Run the transcode worker daemon
    Run {
        /// Worker configuration written by `pair`
        #[arg(long)]
        config: Option<PathBuf>,
        /// FFmpeg binary (default: next to this program, then PATH)
        #[arg(long)]
        ffmpeg: Option<PathBuf>,
        /// Maximum simultaneous encodes (default: 3 with a GPU encoder, otherwise 1)
        #[arg(long)]
        max_jobs: Option<u32>,
        /// Take no new work while another program keeps an NVIDIA GPU busy
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        pause_while_gaming: bool,
        /// Directory for in-progress output (default: system temp directory)
        #[arg(long)]
        work_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    match Cli::parse().command {
        Commands::Pair { server, code, name, mac, config, ffmpeg } => {
            let config = config.unwrap_or_else(default_config_path);
            pair(server, code, name, mac, &config, &find_ffmpeg(ffmpeg)).await
        }
        Commands::Run { config, ffmpeg, max_jobs, pause_while_gaming, work_dir } => {
            let config_path = config.unwrap_or_else(default_config_path);
            let content = tokio::fs::read_to_string(&config_path).await.with_context(|| {
                format!("cannot read {}; run `mbl-worker pair` first", config_path.display())
            })?;
            let config: WorkerConfigFile = serde_json::from_str(&content)?;
            let ffmpeg = find_ffmpeg(ffmpeg);
            info!(ffmpeg = %ffmpeg.display(), "testing video encoders");
            let ffmpeg_for_test = ffmpeg.clone();
            let encoders =
                tokio::task::spawn_blocking(move || detect_encoders(&ffmpeg_for_test)).await?;
            let opts = DaemonOptions {
                max_jobs: max_jobs.unwrap_or_else(|| default_max_jobs(&encoders)).max(1),
                encoders,
                ffmpeg,
                pause_while_gaming,
                work_dir: work_dir.unwrap_or_else(|| std::env::temp_dir().join("mbl-worker")),
            };
            let stop = CancellationToken::new();
            let on_signal = stop.clone();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                info!("shutting down");
                on_signal.cancel();
            });
            mbl_worker::run_daemon(config, opts, stop).await
        }
    }
}

/// Prefer an FFmpeg shipped next to the worker (Windows packages bundle one), then PATH.
fn find_ffmpeg(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(path) = explicit {
        return path;
    }
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

async fn pair(
    server: String,
    code: String,
    name: Option<String>,
    mac: Option<String>,
    config_path: &Path,
    ffmpeg: &Path,
) -> Result<()> {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "worker-host".into());
    let worker_name = name.unwrap_or_else(|| format!("Worker-{hostname}"));

    info!("testing video encoders (this can take a minute)");
    let ffmpeg_owned = ffmpeg.to_path_buf();
    let encoders = tokio::task::spawn_blocking(move || detect_encoders(&ffmpeg_owned)).await?;
    if encoders.is_empty() {
        bail!("no working video encoder found with {}", ffmpeg.display());
    }
    let fp = mbl_hwaccel::fingerprint::detect_fingerprint();
    let gpu_device_name = fp
        .nvidia_gpus
        .first()
        .map(|g| g.name.clone())
        .or_else(|| fp.drm_devices.first().and_then(|d| d.render_node.clone()))
        .unwrap_or_else(|| "CPU".into());
    let gpu_vendor = if !fp.nvidia_gpus.is_empty() {
        "Nvidia"
    } else if encoders.iter().any(|e| e.backend == "qsv") {
        "Intel"
    } else if encoders.iter().any(|e| e.backend == "d3d11Amf") {
        "Amd"
    } else if encoders.iter().any(|e| e.backend == "vaapi") {
        "Vaapi"
    } else {
        "Cpu"
    };
    let capabilities = WorkerCapabilities {
        gpu_vendor: gpu_vendor.into(),
        gpu_device_name,
        supported_video_decoders: Vec::new(),
        supported_video_encoders: encoders
            .iter()
            .map(|e| format!("{}/{}", e.backend, e.codec))
            .collect(),
        supports_hardware_tonemap: false,
        max_concurrent_sessions: default_max_jobs(&encoders),
    };
    let req = WorkerPairRequest {
        pairing_code: code,
        name: worker_name.clone(),
        hostname: Some(hostname),
        mac_address: mac,
        capabilities,
        client_cert_fingerprint: None,
    };
    let server = server.trim_end_matches('/').to_owned();
    let resp = reqwest::Client::new()
        .post(format!("{server}/api/v1/workers/pair"))
        .json(&req)
        .send()
        .await
        .context("cannot connect to the MBL server")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("server rejected pairing ({status}): {body}");
    }
    let paired: WorkerPairResponse = resp.json().await.context("invalid pairing response")?;
    let config = WorkerConfigFile {
        server_url: server,
        worker_id: paired.worker_id,
        auth_token: paired.auth_token,
        name: worker_name,
    };
    if let Some(dir) = config_path.parent().filter(|d| !d.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(dir).await?;
    }
    tokio::fs::write(config_path, serde_json::to_string_pretty(&config)?)
        .await
        .with_context(|| format!("cannot write {}", config_path.display()))?;
    info!(worker_id = paired.worker_id.0, path = %config_path.display(), "paired; start with `mbl-worker run`");
    Ok(())
}
