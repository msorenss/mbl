//! Persistent background job queue and runner.
//!
//! Jobs are rows in the `jobs` table, so they survive restarts. A single runner task claims jobs
//! atomically, respects per-kind concurrency limits and priorities, retries with exponential
//! backoff, and pauses idle-priority work while media is playing.

mod runner;

use std::sync::Arc;

use async_trait::async_trait;
use mbl_core::events::{Event, EventBus};
use mbl_core::{JobId, Result, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::jobs::JobsRepo;
use mbl_platform::PlatformProfile;
use tokio_util::sync::CancellationToken;

pub use mbl_db::repo::jobs::Job;
pub use runner::{JobQueue, JobRunner, RunnerConfig};

/// Job kind names shared by producers and handlers.
pub mod kinds {
    pub const LIBRARY_SCAN: &str = "library.scan";
    pub const METADATA_MATCH: &str = "metadata.match";
    pub const METADATA_REFRESH: &str = "metadata.refresh";
    pub const ARTWORK_FETCH: &str = "artwork.fetch";
    pub const MEDIA_OPTIMIZE: &str = "media.optimize";
    pub const MEDIA_TRICKPLAY: &str = "media.trickplay";
    pub const MEDIA_INTRO_DETECT: &str = "media.intro_detect";
    pub const MEDIA_CREDITS_DETECT: &str = "media.credits_detect";
    pub const MEDIA_SCENE_INDEX: &str = "media.scene_index";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// User-triggered work.
    High = 0,
    Normal = 1,
    /// Background work; paused while any playback session is active or SoC temperature >= 75 °C.
    Idle = 2,
}

#[async_trait]
pub trait JobHandler: Send + Sync + 'static {
    /// Stable kind name stored in the DB, e.g. `"library.scan"`.
    fn kind(&self) -> &'static str;

    /// Max concurrently running jobs of this kind.
    fn concurrency(&self, _profile: PlatformProfile) -> usize {
        1
    }

    async fn run(&self, ctx: JobCtx, payload: serde_json::Value) -> Result<()>;
}

/// Reports whether media is playing or the host is thermally throttled so idle jobs can yield (M2.7, M5.1).
pub trait ActivityProbe: Send + Sync + 'static {
    fn playback_active(&self) -> bool;

    /// True if SoC temperature is >= 75 °C or active thermal throttling is detected (M5.1).
    fn thermal_throttled(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoActivity;

impl ActivityProbe for NoActivity {
    fn playback_active(&self) -> bool {
        false
    }
}

pub struct JobCtx {
    pub id: JobId,
    pub db: Db,
    pub events: EventBus,
    /// Cancelled on user cancel or server shutdown; long jobs should check it regularly.
    pub cancel: CancellationToken,
    pub progress: ProgressReporter,
}

#[derive(Debug, Clone)]
pub struct ProgressReporter {
    id: JobId,
    kind: &'static str,
    repo: JobsRepo,
    events: EventBus,
}

impl ProgressReporter {
    /// Record progress in `0.0..=1.0`. Callers should throttle (e.g. every 1%).
    pub async fn report(&self, progress: f32) -> Result<()> {
        let progress = progress.clamp(0.0, 1.0);
        self.repo.set_progress(self.id, f64::from(progress), now_ms()).await?;
        self.events.publish(Event::JobUpdated {
            job_id: self.id,
            kind: self.kind.to_owned(),
            status: mbl_db::repo::jobs::RUNNING.to_owned(),
            progress,
        });
        Ok(())
    }
}

pub(crate) type Handlers = std::collections::HashMap<&'static str, Arc<dyn JobHandler>>;
