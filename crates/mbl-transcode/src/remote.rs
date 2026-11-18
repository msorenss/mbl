//! Remote transcode dispatch: hands HLS encode jobs to paired `mbl-worker` machines.
//!
//! Workers long-poll for [`WorkerMessage`]s. A job carries the server's typed [`Pipeline`];
//! the worker runs FFmpeg itself and uploads `init.mp4`, numbered fragments and its EVENT
//! playlist (`ffmpeg.m3u8`) into the session directory, exactly the files a local FFmpeg
//! would write. [`crate::HlsSession`] therefore reads remote progress like local progress
//! and falls back to local encoding when a job fails or its worker disappears.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use mbl_core::{
    Error, PartId, Result, WorkerId, WorkerJob, WorkerJobOutcome, WorkerJobReport, WorkerMessage,
    WorkerPollRequest,
};
use tokio::sync::Notify;

use crate::pipeline::{Backend, Pipeline, SubtitleOp, VideoCodec, VideoOp};

/// A worker that has not polled or sent a heartbeat for this long is treated as gone.
pub const WORKER_LOST_AFTER: Duration = Duration::from_secs(15);
/// Largest file a worker may upload (one fragment, init segment or playlist).
pub const MAX_UPLOAD_BYTES: usize = 256 * 1024 * 1024;

/// Signs a short-lived source URL token for `(worker, part)`.
pub type SourceSigner = Arc<dyn Fn(WorkerId, PartId) -> String + Send + Sync>;

/// Current state of a remote job as seen by the owning HLS session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteJobState {
    Running,
    Finished,
    Failed(String),
}

/// A started remote job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAssignment {
    pub job_id: String,
    pub worker_id: WorkerId,
    pub backend: Backend,
}

/// Public summary of a connected worker for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteWorkerSummary {
    pub worker_id: WorkerId,
    pub online: bool,
    pub paused: bool,
    pub active_jobs: usize,
    pub max_jobs: u32,
}

struct WorkerSlot {
    last_seen: Instant,
    encoders: HashSet<(Backend, VideoCodec)>,
    max_jobs: u32,
    paused: bool,
    outbox: Vec<WorkerMessage>,
    notify: Arc<Notify>,
}

struct Job {
    worker_id: WorkerId,
    dir: Utf8PathBuf,
    renditions: Vec<String>,
    state: RemoteJobState,
}

#[derive(Default)]
struct Inner {
    workers: HashMap<WorkerId, WorkerSlot>,
    jobs: HashMap<String, Job>,
}

/// In-memory job broker shared by the HLS session manager and the worker API.
pub struct RemoteDispatcher {
    signer: SourceSigner,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for RemoteDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteDispatcher").finish_non_exhaustive()
    }
}

impl RemoteDispatcher {
    pub fn new(signer: SourceSigner) -> Self {
        Self { signer, inner: Mutex::default() }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Record a poll: refresh capabilities, reconcile running jobs and return the wake-up handle.
    pub fn register_poll(&self, worker_id: WorkerId, req: &WorkerPollRequest) -> Arc<Notify> {
        let encoders = req
            .encoders
            .iter()
            .filter_map(|e| Some((e.backend.parse().ok()?, e.codec.parse().ok()?)))
            .filter(|(backend, _): &(Backend, VideoCodec)| !backend.is_pi())
            .collect();
        let mut inner = self.lock();
        let slot = inner.workers.entry(worker_id).or_insert_with(|| WorkerSlot {
            last_seen: Instant::now(),
            encoders: HashSet::new(),
            max_jobs: 1,
            paused: false,
            outbox: Vec::new(),
            notify: Arc::new(Notify::new()),
        });
        slot.last_seen = Instant::now();
        slot.encoders = encoders;
        slot.max_jobs = req.max_jobs.clamp(1, 16);
        slot.paused = req.paused;
        let notify = slot.notify.clone();
        // A job the worker no longer runs (crash, restart) has failed unless it was already settled.
        let active: HashSet<&str> = req.active_job_ids.iter().map(String::as_str).collect();
        let pending: HashSet<String> = inner
            .workers
            .get(&worker_id)
            .map(|slot| {
                slot.outbox
                    .iter()
                    .filter_map(|m| match m {
                        WorkerMessage::Start { job } => Some(job.job_id.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (id, job) in &mut inner.jobs {
            if job.worker_id == worker_id
                && job.state == RemoteJobState::Running
                && !active.contains(id.as_str())
                && !pending.contains(id)
            {
                job.state = RemoteJobState::Failed("worker is no longer running the job".into());
            }
        }
        notify
    }

    /// Mark a worker alive (heartbeat).
    pub fn touch(&self, worker_id: WorkerId) {
        if let Some(slot) = self.lock().workers.get_mut(&worker_id) {
            slot.last_seen = Instant::now();
        }
    }

    /// Drain queued messages for a worker.
    pub fn take_messages(&self, worker_id: WorkerId) -> Vec<WorkerMessage> {
        let mut inner = self.lock();
        let Some(slot) = inner.workers.get_mut(&worker_id) else {
            return Vec::new();
        };
        slot.last_seen = Instant::now();
        std::mem::take(&mut slot.outbox)
    }

    /// Forget a revoked worker and fail its jobs.
    pub fn remove_worker(&self, worker_id: WorkerId) {
        let mut inner = self.lock();
        if let Some(slot) = inner.workers.remove(&worker_id) {
            slot.notify.notify_one();
        }
        for job in inner.jobs.values_mut() {
            if job.worker_id == worker_id && job.state == RemoteJobState::Running {
                job.state = RemoteJobState::Failed("worker was removed".into());
            }
        }
    }

    /// Whether a pipeline may run remotely at all.
    pub fn eligible(pipeline: &Pipeline) -> Option<VideoCodec> {
        let VideoOp::Encode { codec, .. } = &pipeline.video else {
            return None;
        };
        // Text subtitle burn-in reads the server's file or re-opens the input by path.
        if matches!(
            pipeline.subtitle,
            SubtitleOp::BurnText { .. } | SubtitleOp::BurnTextFile { .. }
        ) {
            return None;
        }
        Some(*codec)
    }

    fn pick(
        inner: &Inner,
        codec: VideoCodec,
        renditions: usize,
        now: Instant,
    ) -> Option<(WorkerId, Backend)> {
        let mut best: Option<(WorkerId, Backend, usize)> = None;
        for (&id, slot) in &inner.workers {
            if slot.paused || now.saturating_duration_since(slot.last_seen) > WORKER_LOST_AFTER {
                continue;
            }
            let running = inner
                .jobs
                .values()
                .filter(|j| j.worker_id == id && j.state == RemoteJobState::Running)
                .count();
            if running >= slot.max_jobs as usize {
                continue;
            }
            // Prefer hardware encoders; software only for a single rendition.
            let backend = [Backend::Nvenc, Backend::Qsv, Backend::Vaapi, Backend::D3d11Amf]
                .into_iter()
                .find(|b| slot.encoders.contains(&(*b, codec)))
                .or_else(|| {
                    (renditions <= 1 && slot.encoders.contains(&(Backend::Software, codec)))
                        .then_some(Backend::Software)
                });
            let Some(backend) = backend else { continue };
            let better = match best {
                None => true,
                Some((_, b, r)) => {
                    (backend.is_gpu() && !b.is_gpu())
                        || (backend.is_gpu() == b.is_gpu() && running < r)
                }
            };
            if better {
                best = Some((id, backend, running));
            }
        }
        best.map(|(id, backend, _)| (id, backend))
    }

    /// Whether some online worker could take this pipeline now.
    pub fn can_accept(&self, pipeline: &Pipeline) -> bool {
        let Some(codec) = Self::eligible(pipeline) else {
            return false;
        };
        let inner = self.lock();
        Self::pick(&inner, codec, pipeline.output.renditions.len().max(1), Instant::now()).is_some()
    }

    /// Hand `pipeline` (already positioned at its start segment) to the best available worker.
    pub fn start(&self, part_id: PartId, pipeline: &Pipeline) -> Option<RemoteAssignment> {
        let codec = Self::eligible(pipeline)?;
        let renditions: Vec<String> = if pipeline.output.renditions.is_empty() {
            vec!["0".to_owned()]
        } else {
            pipeline.output.renditions.iter().map(|r| r.name.clone()).collect()
        };
        let job_id = new_job_id().ok()?;
        let mut inner = self.lock();
        let (worker_id, backend) = Self::pick(&inner, codec, renditions.len(), Instant::now())?;
        let mut remote = pipeline.clone();
        remote.backend = backend;
        // The worker substitutes its own paths; never reveal server paths.
        remote.input = Utf8PathBuf::from("source");
        remote.output.dir = Utf8PathBuf::from("output");
        let pipeline_json = serde_json::to_value(&remote).ok()?;
        let token = (self.signer)(worker_id, part_id);
        let job = WorkerJob {
            job_id: job_id.clone(),
            source_path: format!("/api/v1/stream/worker/source/{}?token={token}", part_id.0),
            renditions: renditions.clone(),
            pipeline: pipeline_json,
        };
        inner.jobs.insert(
            job_id.clone(),
            Job {
                worker_id,
                dir: pipeline.output.dir.clone(),
                renditions,
                state: RemoteJobState::Running,
            },
        );
        let slot = inner.workers.get_mut(&worker_id)?;
        slot.outbox.push(WorkerMessage::Start { job });
        slot.notify.notify_one();
        tracing::info!(
            job_id,
            worker_id = worker_id.0,
            backend = backend.as_str(),
            "assigned HLS encode to transcode worker"
        );
        Some(RemoteAssignment { job_id, worker_id, backend })
    }

    fn send(&self, job_id: &str, make: impl FnOnce(String) -> WorkerMessage, remove: bool) {
        let mut inner = self.lock();
        let worker_id = if remove {
            inner.jobs.remove(job_id).map(|job| job.worker_id)
        } else {
            inner.jobs.get(job_id).map(|job| job.worker_id)
        };
        let Some(worker_id) = worker_id else {
            return;
        };
        if let Some(slot) = inner.workers.get_mut(&worker_id) {
            // A queued but undelivered start can simply be withdrawn.
            if remove {
                let before = slot.outbox.len();
                slot.outbox
                    .retain(|m| !matches!(m, WorkerMessage::Start { job } if job.job_id == job_id));
                if slot.outbox.len() != before {
                    return;
                }
            }
            slot.outbox.push(make(job_id.to_owned()));
            slot.notify.notify_one();
        }
    }

    /// Cancel a job; later uploads for it are rejected.
    pub fn stop(&self, job_id: &str) {
        self.send(job_id, |job_id| WorkerMessage::Stop { job_id }, true);
    }

    pub fn pause(&self, job_id: &str) {
        self.send(job_id, |job_id| WorkerMessage::Pause { job_id }, false);
    }

    pub fn resume(&self, job_id: &str) {
        self.send(job_id, |job_id| WorkerMessage::Resume { job_id }, false);
    }

    /// Current state; a job whose worker went silent is reported as failed.
    pub fn state(&self, job_id: &str) -> RemoteJobState {
        let mut inner = self.lock();
        let worker_id = match inner.jobs.get(job_id) {
            None => return RemoteJobState::Failed("job is unknown".into()),
            Some(job) if job.state != RemoteJobState::Running => return job.state.clone(),
            Some(job) => job.worker_id,
        };
        let alive = inner.workers.get(&worker_id).is_some_and(|slot| {
            Instant::now().saturating_duration_since(slot.last_seen) <= WORKER_LOST_AFTER
        });
        if alive {
            return RemoteJobState::Running;
        }
        let failed = RemoteJobState::Failed("transcode worker stopped responding".into());
        if let Some(job) = inner.jobs.get_mut(job_id) {
            job.state = failed.clone();
        }
        failed
    }

    /// Accept a worker's final report.
    pub fn report(
        &self,
        worker_id: WorkerId,
        job_id: &str,
        report: &WorkerJobReport,
    ) -> Result<()> {
        let mut inner = self.lock();
        let job = inner.jobs.get_mut(job_id).ok_or(Error::NotFound)?;
        if job.worker_id != worker_id {
            return Err(Error::Forbidden);
        }
        if job.state == RemoteJobState::Running {
            job.state = match report.outcome {
                WorkerJobOutcome::Finished => RemoteJobState::Finished,
                WorkerJobOutcome::Failed => {
                    let reason: String = report
                        .message
                        .as_deref()
                        .unwrap_or("worker FFmpeg failed")
                        .chars()
                        .filter(|c| !c.is_control() || *c == '\n')
                        .take(500)
                        .collect();
                    tracing::warn!(job_id, worker_id = worker_id.0, %reason, "transcode worker job failed");
                    RemoteJobState::Failed(reason)
                }
            };
        }
        Ok(())
    }

    /// Store one uploaded file into the job's session directory.
    ///
    /// Only `init.mp4`, `<n>.m4s` and `ffmpeg.m3u8` inside a rendition of a running job owned by
    /// `worker_id` are accepted. The final rename happens under the dispatcher lock so a file from a
    /// job that was just stopped (for example by a seek) can never replace the new job's files.
    pub async fn store_upload(
        &self,
        worker_id: WorkerId,
        job_id: &str,
        rendition: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<()> {
        if data.len() > MAX_UPLOAD_BYTES {
            return Err(Error::Validation("upload is too large".into()));
        }
        let allowed = filename == "init.mp4"
            || filename == "ffmpeg.m3u8"
            || filename.strip_suffix(".m4s").is_some_and(|n| {
                !n.is_empty() && n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit())
            });
        if !allowed {
            return Err(Error::Validation("unexpected upload file name".into()));
        }
        let dir = {
            let inner = self.lock();
            let job = inner.jobs.get(job_id).ok_or(Error::NotFound)?;
            if job.worker_id != worker_id {
                return Err(Error::Forbidden);
            }
            if job.state != RemoteJobState::Running {
                return Err(Error::NotFound);
            }
            if !job.renditions.iter().any(|r| r == rendition) {
                return Err(Error::Validation("unknown rendition".into()));
            }
            job.dir.join(rendition)
        };
        tokio::fs::create_dir_all(dir.as_std_path()).await?;
        let mut suffix = [0u8; 6];
        getrandom::fill(&mut suffix).map_err(|e| Error::Internal(e.to_string().into()))?;
        let tmp = dir.join(format!(
            ".upload-{}.tmp",
            suffix.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ));
        tokio::fs::write(tmp.as_std_path(), data).await?;
        let target = dir.join(filename);
        let result = {
            let inner = self.lock();
            let still_current = inner.jobs.get(job_id).is_some_and(|job| {
                job.worker_id == worker_id && job.state == RemoteJobState::Running
            });
            if still_current {
                std::fs::rename(tmp.as_std_path(), target.as_std_path()).map_err(Error::from)
            } else {
                Err(Error::NotFound)
            }
        };
        if result.is_err() {
            let _ = tokio::fs::remove_file(tmp.as_std_path()).await;
        }
        result
    }

    /// Connected workers for diagnostics.
    pub fn workers(&self) -> Vec<RemoteWorkerSummary> {
        let inner = self.lock();
        let now = Instant::now();
        let mut out: Vec<_> = inner
            .workers
            .iter()
            .map(|(&worker_id, slot)| RemoteWorkerSummary {
                worker_id,
                online: now.saturating_duration_since(slot.last_seen) <= WORKER_LOST_AFTER,
                paused: slot.paused,
                active_jobs: inner
                    .jobs
                    .values()
                    .filter(|j| j.worker_id == worker_id && j.state == RemoteJobState::Running)
                    .count(),
                max_jobs: slot.max_jobs,
            })
            .collect();
        out.sort_by_key(|w| w.worker_id.0);
        out
    }

    /// Drop settled job records (the owning session has seen the outcome or is gone).
    pub fn forget(&self, job_id: &str) {
        let mut inner = self.lock();
        if inner.jobs.get(job_id).is_some_and(|job| job.state != RemoteJobState::Running) {
            inner.jobs.remove(job_id);
        }
    }
}

fn new_job_id() -> Result<String> {
    let mut bytes = [0u8; 12];
    getrandom::fill(&mut bytes).map_err(|e| Error::Internal(e.to_string().into()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{AudioOp, HlsOutput, Rendition};
    use mbl_core::WorkerEncoder;

    fn pipeline(dir: &Utf8PathBuf) -> Pipeline {
        Pipeline {
            input: "/media/movie.mkv".into(),
            start_ms: 0,
            video: VideoOp::Encode {
                codec: VideoCodec::H264,
                width: Some(1280),
                max_bitrate: 4_000_000,
                max_frame_rate: None,
                bounds: None,
                deinterlace: false,
                tonemap: false,
            },
            audio: AudioOp::None,
            subtitle: SubtitleOp::None,
            output: HlsOutput {
                dir: dir.clone(),
                segment_secs: 6,
                start_number: 0,
                renditions: vec![Rendition {
                    name: "0".into(),
                    width: Some(1280),
                    max_bitrate: 4_000_000,
                }],
            },
            backend: Backend::PiDrmDecode,
        }
    }

    fn poll(encoders: &[(&str, &str)], active: &[&str]) -> WorkerPollRequest {
        WorkerPollRequest {
            encoders: encoders
                .iter()
                .map(|(b, c)| WorkerEncoder { backend: (*b).into(), codec: (*c).into() })
                .collect(),
            max_jobs: 1,
            paused: false,
            active_job_ids: active.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn dispatcher() -> RemoteDispatcher {
        RemoteDispatcher::new(Arc::new(|w, p| format!("tok-{}-{}", w.0, p.0)))
    }

    #[test]
    fn assigns_to_matching_gpu_worker_and_hides_server_paths() {
        let d = dispatcher();
        let dir = Utf8PathBuf::from("/srv/transcode/abc");
        assert!(d.start(PartId(7), &pipeline(&dir)).is_none(), "no worker yet");
        d.register_poll(WorkerId(1), &poll(&[("software", "h264")], &[]));
        d.register_poll(WorkerId(2), &poll(&[("nvenc", "h264"), ("software", "h264")], &[]));
        let a = d.start(PartId(7), &pipeline(&dir)).expect("assigned");
        assert_eq!((a.worker_id, a.backend), (WorkerId(2), Backend::Nvenc));
        let msgs = d.take_messages(WorkerId(2));
        let WorkerMessage::Start { job } = &msgs[0] else { panic!("start expected") };
        assert_eq!(job.source_path, "/api/v1/stream/worker/source/7?token=tok-2-7");
        let text = job.pipeline.to_string();
        assert!(!text.contains("/srv/transcode") && !text.contains("/media/movie.mkv"), "{text}");
        assert!(text.contains("nvenc"), "{text}");
        // Worker 2 is now full; the next job goes to the software worker.
        let b = d.start(PartId(8), &pipeline(&dir)).expect("second assigned");
        assert_eq!((b.worker_id, b.backend), (WorkerId(1), Backend::Software));
    }

    #[test]
    fn rejects_ineligible_pipelines_and_unsupported_codecs() {
        let d = dispatcher();
        d.register_poll(WorkerId(1), &poll(&[("nvenc", "hevc")], &[]));
        let dir = Utf8PathBuf::from("/t");
        assert!(!d.can_accept(&pipeline(&dir)), "h264 not offered");
        let mut copy = pipeline(&dir);
        copy.video = VideoOp::Copy;
        assert!(RemoteDispatcher::eligible(&copy).is_none());
        let mut burn = pipeline(&dir);
        burn.subtitle = SubtitleOp::BurnText { index: 0 };
        assert!(RemoteDispatcher::eligible(&burn).is_none());
        // A Pi backend reported by a worker is never trusted for remote work.
        d.register_poll(WorkerId(2), &poll(&[("piDrmDecode", "h264")], &[]));
        assert!(!d.can_accept(&pipeline(&dir)));
    }

    #[test]
    fn job_fails_when_worker_drops_it_or_reports_failure() {
        let d = dispatcher();
        let dir = Utf8PathBuf::from("/t");
        d.register_poll(WorkerId(1), &poll(&[("software", "h264")], &[]));
        let a = d.start(PartId(1), &pipeline(&dir)).unwrap();
        // Still queued (not yet delivered): not failed by a poll that omits it.
        d.register_poll(WorkerId(1), &poll(&[("software", "h264")], &[]));
        assert_eq!(d.state(&a.job_id), RemoteJobState::Running);
        d.take_messages(WorkerId(1));
        d.register_poll(WorkerId(1), &poll(&[("software", "h264")], &[&a.job_id]));
        assert_eq!(d.state(&a.job_id), RemoteJobState::Running);
        d.register_poll(WorkerId(1), &poll(&[("software", "h264")], &[]));
        assert!(matches!(d.state(&a.job_id), RemoteJobState::Failed(_)));

        let b = d.start(PartId(1), &pipeline(&dir)).unwrap();
        let report = WorkerJobReport {
            outcome: WorkerJobOutcome::Failed,
            message: Some("nvenc\u{0}boom".into()),
        };
        assert!(matches!(d.report(WorkerId(9), &b.job_id, &report), Err(Error::Forbidden)));
        d.report(WorkerId(1), &b.job_id, &report).unwrap();
        assert_eq!(d.state(&b.job_id), RemoteJobState::Failed("nvencboom".into()));
    }

    #[tokio::test]
    async fn uploads_are_confined_to_running_jobs_of_the_owner() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
        let d = dispatcher();
        d.register_poll(WorkerId(1), &poll(&[("software", "h264")], &[]));
        let a = d.start(PartId(1), &pipeline(&dir)).unwrap();
        d.store_upload(WorkerId(1), &a.job_id, "0", "3.m4s", b"frag").await.unwrap();
        assert_eq!(std::fs::read(dir.join("0/3.m4s")).unwrap(), b"frag");
        for (worker, rendition, name) in [
            (WorkerId(2), "0", "4.m4s"),
            (WorkerId(1), "1", "4.m4s"),
            (WorkerId(1), "0", "../4.m4s"),
            (WorkerId(1), "0", "master.m3u8"),
            (WorkerId(1), "0", "x.m4s"),
        ] {
            assert!(
                d.store_upload(worker, &a.job_id, rendition, name, b"x").await.is_err(),
                "{name}"
            );
        }
        d.stop(&a.job_id);
        assert!(d.store_upload(WorkerId(1), &a.job_id, "0", "4.m4s", b"late").await.is_err());
        assert!(!dir.join("0/4.m4s").exists());
        let leftovers: Vec<_> =
            std::fs::read_dir(dir.join("0")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    }
}
