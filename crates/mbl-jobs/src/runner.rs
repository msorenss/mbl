use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mbl_core::events::{Event, EventBus};
use mbl_core::{JobId, Result, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::jobs::{self, Job, JobsRepo};
use mbl_platform::PlatformProfile;
use tokio::sync::Notify;
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{ActivityProbe, Handlers, JobCtx, JobHandler, NoActivity, Priority, ProgressReporter};

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Fallback poll interval (enqueue also wakes the runner immediately).
    pub poll_interval: Duration,
    /// First retry delay; doubles per attempt.
    pub base_backoff: Duration,
    pub max_attempts: i64,
    pub profile: PlatformProfile,
    /// How long shutdown waits for running jobs.
    pub shutdown_grace: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            base_backoff: Duration::from_secs(10),
            max_attempts: 5,
            profile: PlatformProfile::Generic,
            shutdown_grace: Duration::from_secs(10),
        }
    }
}

/// Handle for enqueueing and cancelling jobs. Cheap to clone.
#[derive(Debug, Clone)]
pub struct JobQueue {
    repo: JobsRepo,
    notify: Arc<Notify>,
    cancels: Arc<Mutex<HashMap<JobId, CancellationToken>>>,
    events: EventBus,
}

impl JobQueue {
    /// Enqueue a job. If an identical job (kind + payload) is already queued or running, its ID
    /// is returned instead of creating a duplicate.
    pub async fn enqueue(
        &self,
        kind: &str,
        payload: &serde_json::Value,
        priority: Priority,
    ) -> Result<JobId> {
        let payload = payload.to_string();
        if let Some(existing) = self.repo.find_active(kind, &payload).await? {
            return Ok(existing);
        }
        let id = self.repo.insert(kind, &payload, priority as i64, now_ms()).await?;
        self.events.publish(Event::JobUpdated {
            job_id: id,
            kind: kind.to_owned(),
            status: jobs::QUEUED.to_owned(),
            progress: 0.0,
        });
        self.notify.notify_one();
        Ok(id)
    }

    /// Cancel a queued or running job. Returns false if it already finished.
    pub async fn cancel(&self, id: JobId) -> Result<bool> {
        if self.repo.cancel_queued(id, now_ms()).await? {
            return Ok(true);
        }
        let token = lock(&self.cancels).get(&id).cloned();
        Ok(token.map(|t| t.cancel()).is_some())
    }

    pub async fn get(&self, id: JobId) -> Result<Option<Job>> {
        self.repo.get(id).await
    }

    pub async fn list_recent(&self, limit: i64) -> Result<Vec<Job>> {
        self.repo.list_recent(limit).await
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct JobRunner {
    db: Db,
    queue: JobQueue,
    handlers: Handlers,
    config: RunnerConfig,
    activity: Arc<dyn ActivityProbe>,
}

impl JobRunner {
    pub fn new(db: Db, events: EventBus, config: RunnerConfig) -> Self {
        let queue = JobQueue {
            repo: JobsRepo::new(db.clone()),
            notify: Arc::new(Notify::new()),
            cancels: Arc::default(),
            events,
        };
        Self { db, queue, handlers: HashMap::new(), config, activity: Arc::new(NoActivity) }
    }

    pub fn queue(&self) -> JobQueue {
        self.queue.clone()
    }

    pub fn register(&mut self, handler: Arc<dyn JobHandler>) {
        self.handlers.insert(handler.kind(), handler);
    }

    pub fn set_activity_probe(&mut self, probe: Arc<dyn ActivityProbe>) {
        self.activity = probe;
    }

    /// Re-queue jobs interrupted by a crash, then run until `shutdown` is cancelled.
    pub async fn start(self, shutdown: CancellationToken) -> Result<JoinHandle<()>> {
        let recovered = self.queue.repo.requeue_running(now_ms()).await?;
        if recovered > 0 {
            tracing::info!(recovered, "re-queued interrupted jobs");
        }
        Ok(tokio::spawn(self.run(shutdown)))
    }

    async fn run(self, shutdown: CancellationToken) {
        let limits: HashMap<&'static str, usize> = self
            .handlers
            .iter()
            .map(|(k, h)| (*k, h.concurrency(self.config.profile).max(1)))
            .collect();
        let mut running: HashMap<&'static str, usize> = HashMap::new();
        let mut tasks: JoinSet<&'static str> = JoinSet::new();

        loop {
            if let Err(e) = self.dispatch(&limits, &mut running, &mut tasks, &shutdown).await {
                tracing::error!(error = %e, "job dispatch failed");
            }
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = self.queue.notify.notified() => {}
                () = tokio::time::sleep(self.config.poll_interval) => {}
                Some(done) = tasks.join_next() => {
                    if let Ok(kind) = done {
                        running.entry(kind).and_modify(|n| *n = n.saturating_sub(1));
                    }
                }
            }
        }

        // Shutdown: running jobs see their tokens cancelled (children of `shutdown`).
        let drained = tokio::time::timeout(self.config.shutdown_grace, async {
            while tasks.join_next().await.is_some() {}
        })
        .await;
        if drained.is_err() {
            tracing::warn!("jobs still running at shutdown; they will be re-queued on next start");
            tasks.abort_all();
        }
    }

    async fn dispatch(
        &self,
        limits: &HashMap<&'static str, usize>,
        running: &mut HashMap<&'static str, usize>,
        tasks: &mut JoinSet<&'static str>,
        shutdown: &CancellationToken,
    ) -> Result<()> {
        loop {
            let free: Vec<&str> = limits
                .iter()
                .filter(|(k, limit)| running.get(*k).copied().unwrap_or(0) < **limit)
                .map(|(k, _)| *k)
                .collect();
            if free.is_empty() {
                return Ok(());
            }
            let pause_idle = self.activity.playback_active() || self.activity.thermal_throttled();
            let max_priority = if pause_idle { Priority::Normal } else { Priority::Idle } as i64;
            let kinds = serde_json::to_string(&free).unwrap_or_else(|_| "[]".into());
            let Some(job) = self.queue.repo.claim_next(&kinds, max_priority, now_ms()).await?
            else {
                return Ok(());
            };
            let Some((&kind, handler)) = self.handlers.get_key_value(job.kind.as_str()) else {
                continue;
            };
            *running.entry(kind).or_default() += 1;
            let token = shutdown.child_token();
            lock(&self.queue.cancels).insert(job.id, token.clone());
            tasks.spawn(execute(
                job,
                kind,
                handler.clone(),
                self.db.clone(),
                self.queue.clone(),
                self.config.clone(),
                token,
                shutdown.clone(),
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute(
    job: Job,
    kind: &'static str,
    handler: Arc<dyn JobHandler>,
    db: Db,
    queue: JobQueue,
    config: RunnerConfig,
    cancel: CancellationToken,
    shutdown: CancellationToken,
) -> &'static str {
    let publish = |status: &str, progress: f32| {
        queue.events.publish(Event::JobUpdated {
            job_id: job.id,
            kind: kind.to_owned(),
            status: status.to_owned(),
            progress,
        });
    };
    publish(jobs::RUNNING, 0.0);
    tracing::debug!(job_id = %job.id, kind, attempt = job.attempts, "job started");

    let ctx = JobCtx {
        id: job.id,
        db,
        events: queue.events.clone(),
        cancel: cancel.clone(),
        progress: ProgressReporter {
            id: job.id,
            kind,
            repo: queue.repo.clone(),
            events: queue.events.clone(),
        },
    };
    let payload = serde_json::from_str(&job.payload).unwrap_or(serde_json::Value::Null);
    let result = handler.run(ctx, payload).await;
    lock(&queue.cancels).remove(&job.id);

    let now = now_ms();
    let outcome = if shutdown.is_cancelled() {
        // Left as `running`; re-queued on next start.
        return kind;
    } else if cancel.is_cancelled() {
        publish(jobs::CANCELLED, job.progress as f32);
        queue.repo.finish(job.id, jobs::CANCELLED, None, now).await
    } else {
        match result {
            Ok(()) => {
                tracing::debug!(job_id = %job.id, kind, "job done");
                publish(jobs::DONE, 1.0);
                queue.repo.finish(job.id, jobs::DONE, None, now).await
            }
            Err(e) if job.attempts >= config.max_attempts => {
                tracing::warn!(job_id = %job.id, kind, error = %e, "job failed permanently");
                publish(jobs::FAILED, job.progress as f32);
                queue.repo.finish(job.id, jobs::FAILED, Some(&e.to_string()), now).await
            }
            Err(e) => {
                let exp = u32::try_from(job.attempts.saturating_sub(1)).unwrap_or(0).min(10);
                let delay = config.base_backoff.saturating_mul(2u32.pow(exp));
                let run_after = now + i64::try_from(delay.as_millis()).unwrap_or(i64::MAX / 2);
                tracing::info!(job_id = %job.id, kind, error = %e, retry_in = ?delay, "job failed, will retry");
                publish(jobs::QUEUED, job.progress as f32);
                let res = queue.repo.retry_later(job.id, &e.to_string(), run_after, now).await;
                // Wake the runner when the retry is due.
                let notify = queue.notify.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    notify.notify_one();
                });
                res
            }
        }
    };
    if let Err(e) = outcome {
        tracing::error!(job_id = %job.id, error = %e, "failed to record job outcome");
    }
    kind
}
