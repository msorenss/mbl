//! MBL transcode worker: runs HLS encodes for a central MBL server.
//!
//! The worker long-polls `POST /api/v1/workers/poll`, runs each assigned typed pipeline with its
//! own FFmpeg (input from the signed source URL, output in a private temp directory) and uploads
//! `init.mp4`, fragments and the EVENT playlist as they appear. It never executes command lines
//! from the server: arguments are built locally by [`mbl_transcode::build_args`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;
use mbl_core::{
    WorkerEncoder, WorkerHeartbeat, WorkerId, WorkerJob, WorkerJobOutcome, WorkerJobReport,
    WorkerMessage, WorkerPollRequest, WorkerPollResponse, WorkerStatus,
};
use mbl_transcode::{Backend, FfmpegCaps, Pipeline, SubtitleOp, VideoOp, build_args};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Persistent worker identity written by `mbl-worker pair`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfigFile {
    pub server_url: String,
    pub worker_id: WorkerId,
    pub auth_token: String,
    pub name: String,
}

/// Runtime options for the daemon.
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    pub ffmpeg: PathBuf,
    pub encoders: Vec<WorkerEncoder>,
    pub max_jobs: u32,
    pub pause_while_gaming: bool,
    pub work_dir: PathBuf,
}

/// Default config location: `%ProgramData%\MBL\worker.json` on Windows, `worker.json` elsewhere.
pub fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    if let Some(dir) = std::env::var_os("ProgramData") {
        return PathBuf::from(dir).join("MBL").join("worker.json");
    }
    PathBuf::from("worker.json")
}

/// Run FFmpeg encode self-tests and return the encoders that actually work on this machine.
pub fn detect_encoders(ffmpeg: &Path) -> Vec<WorkerEncoder> {
    let fp = mbl_hwaccel::fingerprint::detect_fingerprint();
    let probe = mbl_hwaccel::ffmpeg::probe_ffmpeg(ffmpeg);
    let mut encoders: Vec<WorkerEncoder> = mbl_hwaccel::selftest::run_selftest(ffmpeg, &fp, &probe)
        .into_iter()
        .filter(|e| e.operation == "encode" && e.ok)
        .filter(|e| e.backend.parse::<Backend>().is_ok_and(|b| !b.is_pi()))
        .map(|e| WorkerEncoder { backend: e.backend, codec: e.codec })
        .collect();
    encoders.sort_by(|a, b| (&a.backend, &a.codec).cmp(&(&b.backend, &b.codec)));
    encoders.dedup();
    encoders
}

/// A sensible concurrency limit: GPUs handle a few streams, CPUs one.
pub fn default_max_jobs(encoders: &[WorkerEncoder]) -> u32 {
    if encoders.iter().any(|e| e.backend.parse::<Backend>().is_ok_and(Backend::is_gpu)) {
        3
    } else {
        1
    }
}

struct RunningJob {
    cancel: CancellationToken,
    pid: Arc<Mutex<Option<u32>>>,
    #[cfg(windows)]
    suspended: Option<mbl_transcode::windows_throttle::SuspendedThreads>,
}

#[derive(Clone)]
struct Api {
    client: reqwest::Client,
    server: String,
    token: String,
}

impl Api {
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.server, path)
    }

    async fn report(&self, job_id: &str, outcome: WorkerJobOutcome, message: Option<String>) {
        let url = self.url(&format!("/api/v1/workers/jobs/{job_id}/report"));
        let body = WorkerJobReport { outcome, message };
        if let Err(error) = self.client.post(url).bearer_auth(&self.token).json(&body).send().await
        {
            warn!(job_id, %error, "could not report job outcome");
        }
    }

    /// Upload one file; `Ok(false)` means the server no longer wants output for this job.
    async fn upload(
        &self,
        job_id: &str,
        rendition: &str,
        name: &str,
        data: Vec<u8>,
    ) -> Result<bool> {
        let url = self.url(&format!("/api/v1/workers/jobs/{job_id}/files/{rendition}/{name}"));
        let mut last = None;
        for attempt in 0..3u64 {
            match self.client.put(&url).bearer_auth(&self.token).body(data.clone()).send().await {
                Ok(res) if res.status().is_success() => return Ok(true),
                Ok(res) if res.status() == reqwest::StatusCode::NOT_FOUND => return Ok(false),
                Ok(res) => last = Some(anyhow::anyhow!("upload rejected with {}", res.status())),
                Err(error) => last = Some(error.into()),
            }
            tokio::time::sleep(Duration::from_millis(500 * (attempt + 1))).await;
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("upload failed")))
    }
}

/// Run the worker until `stop` is cancelled.
pub async fn run_daemon(
    config: WorkerConfigFile,
    opts: DaemonOptions,
    stop: CancellationToken,
) -> Result<()> {
    if opts.encoders.is_empty() {
        bail!("no working video encoder was found; check the FFmpeg installation");
    }
    let api = Api {
        client: reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?,
        server: config.server_url.trim_end_matches('/').to_owned(),
        token: config.auth_token.clone(),
    };
    std::fs::create_dir_all(&opts.work_dir)
        .with_context(|| format!("cannot create work directory {}", opts.work_dir.display()))?;
    info!(
        name = %config.name,
        worker_id = config.worker_id.0,
        encoders = ?opts.encoders.iter().map(|e| format!("{}/{}", e.backend, e.codec)).collect::<Vec<_>>(),
        max_jobs = opts.max_jobs,
        "MBL transcode worker ready"
    );
    let jobs: Arc<Mutex<HashMap<String, RunningJob>>> = Arc::default();
    let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let heartbeat = tokio::spawn(heartbeat_loop(
        api.clone(),
        config.worker_id,
        jobs.clone(),
        paused.clone(),
        opts.pause_while_gaming,
        stop.clone(),
    ));

    while !stop.is_cancelled() {
        let active: Vec<String> =
            jobs.lock().map(|j| j.keys().cloned().collect()).unwrap_or_default();
        let request = WorkerPollRequest {
            encoders: opts.encoders.clone(),
            max_jobs: opts.max_jobs,
            paused: paused.load(std::sync::atomic::Ordering::Acquire),
            active_job_ids: active,
        };
        let poll = api
            .client
            .post(api.url("/api/v1/workers/poll"))
            .bearer_auth(&api.token)
            .timeout(Duration::from_secs(40))
            .json(&request)
            .send();
        let response = tokio::select! {
            () = stop.cancelled() => break,
            response = poll => response,
        };
        let messages = match response {
            Ok(res) if res.status().is_success() => match res.json::<WorkerPollResponse>().await {
                Ok(body) => body.messages,
                Err(error) => {
                    warn!(%error, "invalid poll response");
                    Vec::new()
                }
            },
            Ok(res) if res.status() == reqwest::StatusCode::UNAUTHORIZED => {
                bail!("the server rejected this worker; pair it again");
            }
            Ok(res) => {
                warn!(status = %res.status(), "poll rejected; retrying");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
            Err(error) => {
                warn!(%error, "cannot reach the MBL server; retrying");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };
        for message in messages {
            handle_message(message, &api, &opts, &jobs);
        }
    }

    let running: Vec<RunningJob> =
        jobs.lock().map(|mut j| j.drain().map(|(_, job)| job).collect()).unwrap_or_default();
    for job in running {
        job.cancel.cancel();
    }
    heartbeat.abort();
    Ok(())
}

fn handle_message(
    message: WorkerMessage,
    api: &Api,
    opts: &DaemonOptions,
    jobs: &Arc<Mutex<HashMap<String, RunningJob>>>,
) {
    match message {
        WorkerMessage::Start { job } => {
            let cancel = CancellationToken::new();
            let pid = Arc::new(Mutex::new(None));
            if let Ok(mut map) = jobs.lock() {
                map.insert(
                    job.job_id.clone(),
                    RunningJob {
                        cancel: cancel.clone(),
                        pid: pid.clone(),
                        #[cfg(windows)]
                        suspended: None,
                    },
                );
            }
            let api = api.clone();
            let opts = opts.clone();
            let jobs = jobs.clone();
            tokio::spawn(async move {
                let job_id = job.job_id.clone();
                let result = run_job(&api, &opts, job, cancel.clone(), pid).await;
                if let Ok(mut map) = jobs.lock() {
                    map.remove(&job_id);
                }
                if cancel.is_cancelled() {
                    debug!(job_id, "job stopped by the server");
                    return;
                }
                match result {
                    Ok(()) => api.report(&job_id, WorkerJobOutcome::Finished, None).await,
                    Err(error) => {
                        warn!(job_id, %error, "job failed");
                        api.report(&job_id, WorkerJobOutcome::Failed, Some(format!("{error:#}")))
                            .await;
                    }
                }
            });
        }
        WorkerMessage::Stop { job_id } => {
            if let Some(job) = jobs.lock().ok().and_then(|mut j| j.remove(&job_id)) {
                job.cancel.cancel();
            }
        }
        WorkerMessage::Pause { job_id } => set_paused(jobs, &job_id, true),
        WorkerMessage::Resume { job_id } => set_paused(jobs, &job_id, false),
    }
}

fn set_paused(jobs: &Arc<Mutex<HashMap<String, RunningJob>>>, job_id: &str, pause: bool) {
    let Ok(mut map) = jobs.lock() else { return };
    let Some(job) = map.get_mut(job_id) else { return };
    let Some(pid) = job.pid.lock().ok().and_then(|p| *p) else { return };
    #[cfg(unix)]
    {
        let signal = if pause { "-STOP" } else { "-CONT" };
        let _ = std::process::Command::new("kill")
            .args([signal, &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        if pause {
            if job.suspended.is_none() {
                job.suspended =
                    mbl_transcode::windows_throttle::SuspendedThreads::suspend(pid).ok();
            }
        } else {
            job.suspended = None;
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = (pid, pause);
}

/// Validate an assignment and turn it into a local pipeline.
fn local_pipeline(
    job: &WorkerJob,
    server: &str,
    out_dir: &Path,
    encoders: &[WorkerEncoder],
) -> Result<Pipeline> {
    let mut pipeline: Pipeline = serde_json::from_value(job.pipeline.clone())
        .context("the server sent an unknown pipeline")?;
    let VideoOp::Encode { codec, .. } = &pipeline.video else {
        bail!("only video encodes run on workers");
    };
    if matches!(pipeline.subtitle, SubtitleOp::BurnText { .. } | SubtitleOp::BurnTextFile { .. }) {
        bail!("text subtitle burn-in is not supported on workers");
    }
    if !encoders.iter().any(|e| e.backend == pipeline.backend.as_str() && e.codec == codec.as_str())
    {
        bail!(
            "encoder {}/{} is not available on this worker",
            pipeline.backend.as_str(),
            codec.as_str()
        );
    }
    if !job.source_path.starts_with("/api/v1/stream/worker/source/") {
        bail!("unexpected source path");
    }
    let expected: Vec<String> = if pipeline.output.renditions.is_empty() {
        vec!["0".to_owned()]
    } else {
        pipeline.output.renditions.iter().map(|r| r.name.clone()).collect()
    };
    if expected != job.renditions
        || expected.iter().any(|r| {
            r.is_empty() || !r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
    {
        bail!("rendition list does not match the pipeline");
    }
    pipeline.input = Utf8PathBuf::from(format!("{server}{}", job.source_path));
    pipeline.output.dir = Utf8PathBuf::from_path_buf(out_dir.to_path_buf())
        .map_err(|_| anyhow::anyhow!("work directory is not valid UTF-8"))?;
    Ok(pipeline)
}

async fn run_job(
    api: &Api,
    opts: &DaemonOptions,
    job: WorkerJob,
    cancel: CancellationToken,
    pid_slot: Arc<Mutex<Option<u32>>>,
) -> Result<()> {
    let dir = tempfile::Builder::new().prefix("mbl-job-").tempdir_in(&opts.work_dir)?;
    let pipeline = local_pipeline(&job, &api.server, dir.path(), &opts.encoders)?;
    for rendition in &job.renditions {
        std::fs::create_dir_all(dir.path().join(rendition))?;
    }
    let args = build_args(&pipeline, &FfmpegCaps::default())?;
    info!(
        job_id = job.job_id,
        backend = pipeline.backend.as_str(),
        start_ms = pipeline.start_ms,
        "starting encode"
    );
    let mut child: Child = Command::new(&opts.ffmpeg)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("cannot start FFmpeg")?;
    if let Ok(mut slot) = pid_slot.lock() {
        *slot = child.id();
    }
    let stderr_tail: Arc<Mutex<Vec<String>>> = Arc::default();
    if let Some(stderr) = child.stderr.take() {
        let tail = stderr_tail.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut t) = tail.lock() {
                    t.push(line);
                    if t.len() > 12 {
                        t.remove(0);
                    }
                }
            }
        });
    }

    let mut uploader = Uploader::new(&job);
    let status = loop {
        tokio::select! {
            () = cancel.cancelled() => {
                let _ = child.kill().await;
                return Ok(());
            }
            status = child.wait() => break status?,
            () = tokio::time::sleep(Duration::from_millis(300)) => {
                if !uploader.sync(api, dir.path()).await? {
                    let _ = child.kill().await;
                    cancel.cancel();
                    return Ok(());
                }
            }
        }
    };
    // Publish whatever FFmpeg finished before exiting.
    if !uploader.sync(api, dir.path()).await? {
        cancel.cancel();
        return Ok(());
    }
    if status.success() {
        Ok(())
    } else {
        let tail = stderr_tail.lock().map(|t| t.join("\n")).unwrap_or_default();
        bail!("FFmpeg exited with {status}: {tail}")
    }
}

/// Uploads new output in the order the server expects: init, fragments, then the playlist.
struct Uploader {
    job_id: String,
    renditions: Vec<String>,
    playlists: HashMap<String, String>,
    sent: HashMap<String, HashSet<String>>,
}

impl Uploader {
    fn new(job: &WorkerJob) -> Self {
        Self {
            job_id: job.job_id.clone(),
            renditions: job.renditions.clone(),
            playlists: HashMap::new(),
            sent: HashMap::new(),
        }
    }

    /// Returns `false` when the server stopped accepting output for the job.
    async fn sync(&mut self, api: &Api, root: &Path) -> Result<bool> {
        for rendition in self.renditions.clone() {
            let dir = root.join(&rendition);
            let Ok(playlist) = tokio::fs::read_to_string(dir.join("ffmpeg.m3u8")).await else {
                continue;
            };
            if self.playlists.get(&rendition) == Some(&playlist) {
                continue;
            }
            let mut files = Vec::new();
            if playlist.contains("#EXT-X-MAP:URI=\"init.mp4\"") {
                files.push("init.mp4".to_owned());
            }
            files.extend(
                playlist
                    .lines()
                    .map(str::trim)
                    .filter(|l| l.strip_suffix(".m4s").is_some_and(|n| n.parse::<u32>().is_ok()))
                    .map(str::to_owned),
            );
            for name in files {
                if self.sent.get(&rendition).is_some_and(|s| s.contains(&name)) {
                    continue;
                }
                let path = dir.join(&name);
                let data = tokio::fs::read(&path)
                    .await
                    .with_context(|| format!("FFmpeg listed {name} but it is missing"))?;
                if !api.upload(&self.job_id, &rendition, &name, data).await? {
                    return Ok(false);
                }
                if name != "init.mp4" {
                    let _ = tokio::fs::remove_file(&path).await;
                }
                self.sent.entry(rendition.clone()).or_default().insert(name);
            }
            if !api
                .upload(&self.job_id, &rendition, "ffmpeg.m3u8", playlist.clone().into_bytes())
                .await?
            {
                return Ok(false);
            }
            self.playlists.insert(rendition, playlist);
        }
        Ok(true)
    }
}

async fn heartbeat_loop(
    api: Api,
    worker_id: WorkerId,
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
    paused: Arc<std::sync::atomic::AtomicBool>,
    pause_while_gaming: bool,
    stop: CancellationToken,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            _ = interval.tick() => {}
        }
        let active: Vec<String> =
            jobs.lock().map(|j| j.keys().cloned().collect()).unwrap_or_default();
        let gpu = gpu_utilization().await;
        // Another program is using the GPU heavily while we are idle: hold new work.
        let gaming = pause_while_gaming && active.is_empty() && gpu.is_some_and(|g| g >= 60.0);
        paused.store(gaming, std::sync::atomic::Ordering::Release);
        let state = if gaming {
            "paused_gaming"
        } else if active.is_empty() {
            "idle"
        } else {
            "busy"
        };
        let hb = WorkerHeartbeat {
            worker_id,
            status: WorkerStatus {
                state: state.to_owned(),
                active_sessions: u32::try_from(active.len()).unwrap_or(u32::MAX),
                // CPU load is not measured; GPU load is reported when nvidia-smi is available.
                cpu_usage_percent: 0.0,
                gpu_usage_percent: gpu,
            },
            active_job_ids: active,
        };
        match api
            .client
            .post(api.url("/api/v1/workers/heartbeat"))
            .bearer_auth(&api.token)
            .timeout(Duration::from_secs(10))
            .json(&hb)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {}
            Ok(res) => warn!(status = %res.status(), "heartbeat rejected"),
            Err(error) => error!(%error, "heartbeat failed"),
        }
    }
}

/// Current NVIDIA GPU utilization in percent, when `nvidia-smi` is installed.
async fn gpu_utilization() -> Option<f32> {
    let output = tokio::time::timeout(
        Duration::from_secs(3),
        Command::new("nvidia-smi")
            .args(["--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<f32>().ok())
        .reduce(f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(pipeline: serde_json::Value, renditions: &[&str]) -> WorkerJob {
        WorkerJob {
            job_id: "j1".into(),
            source_path: "/api/v1/stream/worker/source/5?token=t".into(),
            renditions: renditions.iter().map(|s| (*s).to_owned()).collect(),
            pipeline,
        }
    }

    fn encode_pipeline(backend: &str) -> serde_json::Value {
        serde_json::json!({
            "input": "source", "startMs": 12000,
            "video": {"op": "encode", "codec": "h264", "width": 1280, "max_bitrate": 4000000, "deinterlace": false, "tonemap": false},
            "audio": {"op": "none"}, "subtitle": {"op": "none"},
            "output": {"dir": "output", "segmentSecs": 6, "startNumber": 2,
                       "renditions": [{"name": "0", "width": 1280, "maxBitrate": 4000000}]},
            "backend": backend
        })
    }

    #[test]
    fn builds_local_pipeline_only_for_supported_assignments() {
        let sw = [WorkerEncoder { backend: "software".into(), codec: "h264".into() }];
        let out = std::env::temp_dir();
        let p =
            local_pipeline(&job(encode_pipeline("software"), &["0"]), "http://srv:8420", &out, &sw)
                .map_err(|e| format!("{e:#}"))
                .unwrap();
        assert_eq!(p.input.as_str(), "http://srv:8420/api/v1/stream/worker/source/5?token=t");
        assert_eq!(p.output.dir.as_std_path(), out.as_path());
        assert_eq!(p.output.start_number, 2);
        assert!(
            local_pipeline(&job(encode_pipeline("nvenc"), &["0"]), "http://srv", &out, &sw)
                .is_err()
        );
        assert!(
            local_pipeline(&job(encode_pipeline("software"), &["../x"]), "http://srv", &out, &sw)
                .is_err()
        );
        let mut bad = job(encode_pipeline("software"), &["0"]);
        bad.source_path = "/etc/passwd".into();
        assert!(local_pipeline(&bad, "http://srv", &out, &sw).is_err());
    }
}
