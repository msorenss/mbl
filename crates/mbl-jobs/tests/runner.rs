#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use camino::Utf8PathBuf;
use mbl_core::events::EventBus;
use mbl_core::{Error, JobId, Result};
use mbl_db::{Db, DbOptions};
use mbl_jobs::{ActivityProbe, JobCtx, JobHandler, JobQueue, JobRunner, Priority, RunnerConfig};
use mbl_platform::PlatformProfile;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

async fn db() -> (Db, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("mbl.db")).unwrap();
    (
        Db::connect(&path, &DbOptions::default()).await.unwrap(),
        dir,
    )
}

fn config() -> RunnerConfig {
    RunnerConfig {
        poll_interval: Duration::from_millis(50),
        base_backoff: Duration::from_millis(10),
        max_attempts: 3,
        ..Default::default()
    }
}

async fn wait_for_status(queue: &JobQueue, id: JobId, status: &str) -> mbl_jobs::Job {
    for _ in 0..200 {
        let job = queue.get(id).await.unwrap().unwrap();
        if job.status == status {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "job {id} never reached {status}: {:?}",
        queue.get(id).await.unwrap()
    );
}

/// Records payload `n` in order; fails `fail_times` times first; optional sleep.
struct Recorder {
    kind: &'static str,
    order: Arc<Mutex<Vec<i64>>>,
    fail_times: usize,
    calls: AtomicUsize,
    sleep: Duration,
    concurrency: usize,
    active: AtomicUsize,
    max_active: Arc<AtomicUsize>,
}

impl Recorder {
    fn new(kind: &'static str) -> Self {
        Self {
            kind,
            order: Arc::default(),
            fail_times: 0,
            calls: AtomicUsize::new(0),
            sleep: Duration::ZERO,
            concurrency: 1,
            active: AtomicUsize::new(0),
            max_active: Arc::default(),
        }
    }
}

#[async_trait]
impl JobHandler for Recorder {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn concurrency(&self, _: PlatformProfile) -> usize {
        self.concurrency
    }

    async fn run(&self, _ctx: JobCtx, payload: Value) -> Result<()> {
        let now_active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(now_active, Ordering::SeqCst);
        tokio::time::sleep(self.sleep).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        self.order
            .lock()
            .unwrap()
            .push(payload["n"].as_i64().unwrap_or(-1));
        if self.calls.fetch_add(1, Ordering::SeqCst) < self.fail_times {
            return Err(Error::External("transient".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn runs_and_completes() {
    let (db, _dir) = db().await;
    let mut runner = JobRunner::new(db, EventBus::default(), config());
    let rec = Arc::new(Recorder::new("test.record"));
    runner.register(rec.clone());
    let queue = runner.queue();
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();

    let id = queue
        .enqueue("test.record", &json!({ "n": 7 }), Priority::Normal)
        .await
        .unwrap();
    let job = wait_for_status(&queue, id, "done").await;
    assert_eq!(job.progress, 1.0);
    assert_eq!(*rec.order.lock().unwrap(), [7]);
    shutdown.cancel();
}

#[tokio::test]
async fn retries_with_backoff_then_succeeds() {
    let (db, _dir) = db().await;
    let mut runner = JobRunner::new(db, EventBus::default(), config());
    let mut rec = Recorder::new("test.flaky");
    rec.fail_times = 2;
    runner.register(Arc::new(rec));
    let queue = runner.queue();
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();

    let id = queue
        .enqueue("test.flaky", &json!({}), Priority::Normal)
        .await
        .unwrap();
    let job = wait_for_status(&queue, id, "done").await;
    assert_eq!(job.attempts, 3);
    shutdown.cancel();
}

#[tokio::test]
async fn gives_up_after_max_attempts() {
    let (db, _dir) = db().await;
    let mut runner = JobRunner::new(db, EventBus::default(), config());
    let mut rec = Recorder::new("test.broken");
    rec.fail_times = usize::MAX;
    runner.register(Arc::new(rec));
    let queue = runner.queue();
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();

    let id = queue
        .enqueue("test.broken", &json!({}), Priority::Normal)
        .await
        .unwrap();
    let job = wait_for_status(&queue, id, "failed").await;
    assert_eq!(job.attempts, 3);
    assert!(job.error.unwrap().contains("transient"));
    shutdown.cancel();
}

#[tokio::test]
async fn respects_concurrency_limit() {
    let (db, _dir) = db().await;
    let mut runner = JobRunner::new(db, EventBus::default(), config());
    let mut rec = Recorder::new("test.parallel");
    rec.concurrency = 2;
    rec.sleep = Duration::from_millis(100);
    let max_active = rec.max_active.clone();
    runner.register(Arc::new(rec));
    let queue = runner.queue();
    let shutdown = CancellationToken::new();

    let mut ids = Vec::new();
    for n in 0..6 {
        ids.push(
            queue
                .enqueue("test.parallel", &json!({ "n": n }), Priority::Normal)
                .await
                .unwrap(),
        );
    }
    runner.start(shutdown.clone()).await.unwrap();
    for id in ids {
        wait_for_status(&queue, id, "done").await;
    }
    assert_eq!(max_active.load(Ordering::SeqCst), 2);
    shutdown.cancel();
}

#[tokio::test]
async fn runs_by_priority_then_age() {
    let (db, _dir) = db().await;
    let mut runner = JobRunner::new(db, EventBus::default(), config());
    let rec = Arc::new(Recorder::new("test.order"));
    runner.register(rec.clone());
    let queue = runner.queue();

    let mut ids = Vec::new();
    for (n, p) in [
        (1, Priority::Normal),
        (2, Priority::High),
        (3, Priority::Idle),
        (4, Priority::Normal),
    ] {
        ids.push(
            queue
                .enqueue("test.order", &json!({ "n": n }), p)
                .await
                .unwrap(),
        );
    }
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();
    for id in ids {
        wait_for_status(&queue, id, "done").await;
    }
    assert_eq!(*rec.order.lock().unwrap(), [2, 1, 4, 3]);
    shutdown.cancel();
}

#[tokio::test]
async fn requeues_jobs_interrupted_by_a_crash() {
    let (db, _dir) = db().await;
    // Simulate a crash: a job was claimed (running) but never finished.
    let repo = mbl_db::repo::jobs::JobsRepo::new(db.clone());
    let id = repo.insert("test.crash", "{\"n\":1}", 1, 0).await.unwrap();
    let claimed = repo
        .claim_next("[\"test.crash\"]", 2, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((claimed.id, claimed.status.as_str()), (id, "running"));

    let mut runner = JobRunner::new(db, EventBus::default(), config());
    runner.register(Arc::new(Recorder::new("test.crash")));
    let queue = runner.queue();
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();
    let job = wait_for_status(&queue, id, "done").await;
    assert_eq!(job.attempts, 2);
    shutdown.cancel();
}

struct Playing(AtomicBool);

impl ActivityProbe for Playing {
    fn playback_active(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn idle_jobs_wait_for_playback_to_stop() {
    let (db, _dir) = db().await;
    let mut runner = JobRunner::new(db, EventBus::default(), config());
    let playing = Arc::new(Playing(AtomicBool::new(true)));
    runner.set_activity_probe(playing.clone());
    runner.register(Arc::new(Recorder::new("test.idle")));
    let queue = runner.queue();
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();

    let idle = queue
        .enqueue("test.idle", &json!({ "n": 1 }), Priority::Idle)
        .await
        .unwrap();
    let normal = queue
        .enqueue("test.idle", &json!({ "n": 2 }), Priority::Normal)
        .await
        .unwrap();
    wait_for_status(&queue, normal, "done").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(queue.get(idle).await.unwrap().unwrap().status, "queued");

    playing.0.store(false, Ordering::SeqCst);
    wait_for_status(&queue, idle, "done").await;
    shutdown.cancel();
}

struct WaitsForCancel;

#[async_trait]
impl JobHandler for WaitsForCancel {
    fn kind(&self) -> &'static str {
        "test.cancel"
    }

    async fn run(&self, ctx: JobCtx, _: Value) -> Result<()> {
        ctx.progress.report(0.25).await?;
        ctx.cancel.cancelled().await;
        Ok(())
    }
}

#[tokio::test]
async fn cancels_running_and_queued_jobs() {
    let (db, _dir) = db().await;
    let mut runner = JobRunner::new(db, EventBus::default(), config());
    runner.register(Arc::new(WaitsForCancel));
    let queue = runner.queue();
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();

    let running = queue
        .enqueue("test.cancel", &json!({ "n": 1 }), Priority::Normal)
        .await
        .unwrap();
    let waiting = queue
        .enqueue("test.cancel", &json!({ "n": 2 }), Priority::Normal)
        .await
        .unwrap();
    wait_for_status(&queue, running, "running").await;
    assert!(queue.cancel(waiting).await.unwrap());
    assert!(queue.cancel(running).await.unwrap());
    let job = wait_for_status(&queue, running, "cancelled").await;
    assert_eq!(job.progress, 0.25);
    assert_eq!(
        queue.get(waiting).await.unwrap().unwrap().status,
        "cancelled"
    );
    assert!(!queue.cancel(running).await.unwrap());
    shutdown.cancel();
}

#[tokio::test]
async fn identical_active_jobs_are_deduplicated() {
    let (db, _dir) = db().await;
    let runner = JobRunner::new(db, EventBus::default(), config());
    let queue = runner.queue();
    let a = queue
        .enqueue("test.dedup", &json!({ "libraryId": 1 }), Priority::High)
        .await
        .unwrap();
    let b = queue
        .enqueue("test.dedup", &json!({ "libraryId": 1 }), Priority::High)
        .await
        .unwrap();
    let c = queue
        .enqueue("test.dedup", &json!({ "libraryId": 2 }), Priority::High)
        .await
        .unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[tokio::test]
async fn publishes_job_events() {
    let (db, _dir) = db().await;
    let events = EventBus::default();
    let mut rx = events.subscribe();
    let mut runner = JobRunner::new(db, events, config());
    runner.register(Arc::new(Recorder::new("test.events")));
    let queue = runner.queue();
    let shutdown = CancellationToken::new();
    runner.start(shutdown.clone()).await.unwrap();
    let id = queue
        .enqueue("test.events", &json!({}), Priority::Normal)
        .await
        .unwrap();
    wait_for_status(&queue, id, "done").await;

    let mut statuses = Vec::new();
    while let Ok(mbl_core::events::Event::JobUpdated { status, .. }) = rx.try_recv() {
        statuses.push(status);
    }
    assert_eq!(statuses, ["queued", "running", "done"]);
    shutdown.cancel();
}
