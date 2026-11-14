//! Hardware acceleration reporting, functional self-test orchestration, live capacity modeling,
//! and Hailo NPU resource-sharing status (M4.1, M4.3, M4.11, M4.15, M4.16, M4.17).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, JobId, Result};
use mbl_db::Db;
use mbl_db::repo::hw::{self, HwCapabilityRow};
use mbl_db::repo::jobs::{DONE, JobsRepo};
use mbl_db::repo::settings::SettingsRepo;
use mbl_hwaccel::{
    BadBackendFailure, BadBackendTracker, FfmpegProbeCaps, HailoConfig, HailoRuntimeStatus,
    HardwareFingerprint, HwCapEntry, NpuCoordinator, TranscodeCapacity, compute_capacity,
    detect_fingerprint, probe_ffmpeg, run_selftest,
};
use mbl_playback::HwSummary;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::blocking;
use crate::stream::StreamService;

/// Complete hardware acceleration report returned by `GET /api/v1/hw/report`, `get_hw_report`,
/// and `mbl://hw/report` (M4.15, M4.16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HwReport {
    pub fingerprint: HardwareFingerprint,
    pub ffmpeg: FfmpegProbeCaps,
    pub capabilities: Vec<HwCapEntry>,
    pub capacity: TranscodeCapacity,
    pub recent_failures: Vec<BadBackendFailure>,
    pub hailo: HailoRuntimeStatus,
}

/// Response returned when triggering a hardware self-test (`POST /api/v1/hw/test` and `run_hw_selftest`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HwSelftestResponse {
    pub job_id: JobId,
    pub capabilities: Vec<HwCapEntry>,
    pub capacity: TranscodeCapacity,
}

#[derive(Debug, Clone)]
pub struct HwService {
    db: Db,
    stream: StreamService,
    fingerprint: Arc<HardwareFingerprint>,
    tracker: BadBackendTracker,
    npu_coordinator: NpuCoordinator,
}

impl HwService {
    pub fn new(db: Db, stream: StreamService) -> Self {
        let fp = detect_fingerprint();
        Self {
            db,
            stream,
            fingerprint: Arc::new(fp),
            tracker: BadBackendTracker::default(),
            npu_coordinator: NpuCoordinator::new(HailoConfig::default()),
        }
    }

    pub fn with_stream(mut self, stream: StreamService) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_hailo_config(mut self, config: HailoConfig) -> Self {
        self.npu_coordinator = NpuCoordinator::new(config);
        self
    }

    pub fn fingerprint(&self) -> &HardwareFingerprint {
        &self.fingerprint
    }

    pub fn tracker(&self) -> &BadBackendTracker {
        &self.tracker
    }

    pub fn npu_coordinator(&self) -> &NpuCoordinator {
        &self.npu_coordinator
    }

    fn resolve_ffmpeg_bin(&self) -> PathBuf {
        if let Some(mgr) = self.stream.hls_manager() {
            return mgr.config().ffmpeg_bin.clone();
        }
        for candidate in ["/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/bin/ffmpeg"] {
            let p = Path::new(candidate);
            if p.exists() {
                return p.to_path_buf();
            }
        }
        PathBuf::from("ffmpeg")
    }

    fn active_video_sessions(&self) -> usize {
        self.stream.hls_manager().map(|m| m.active_video_transcode_count()).unwrap_or(0)
    }

    async fn sync_hailo_model_overrides(&self) {
        let repo = SettingsRepo::new(self.db.clone());
        let vision = repo.get("hailo_vision_hef_path").await.ok().flatten();
        let vlm = repo.get("hailo_vlm_hef_path").await.ok().flatten();
        let llm = repo.get("hailo_llm_hef_path").await.ok().flatten();
        let whisper = repo.get("hailo_whisper_hef_path").await.ok().flatten();
        let current = mbl_hwaccel::hailo::get_hailo_model_overrides();
        let next = mbl_hwaccel::hailo::HailoModelOverrides {
            vision_hef_path: vision,
            vlm_hef_path: vlm,
            llm_hef_path: llm,
            whisper_hef_path: whisper,
        };
        if current != next {
            mbl_hwaccel::hailo::set_hailo_model_overrides(next);
        }
    }

    /// Return the full [`HwReport`] for any authenticated user (`read` scope).
    pub async fn get_report(&self, _actor: &Actor) -> Result<HwReport> {
        self.sync_hailo_model_overrides().await;
        let ffmpeg_bin = self.resolve_ffmpeg_bin();
        let probe = blocking(move || Ok(probe_ffmpeg(&ffmpeg_bin))).await?;
        let rows = hw::list_all(&self.db).await?;
        let capabilities = rows_to_entries(rows);
        let capacity = compute_capacity(
            &self.fingerprint,
            &probe,
            &capabilities,
            &self.tracker,
            self.active_video_sessions(),
        );
        let recent_failures = self.tracker.recent_failures();
        let hailo = self.npu_coordinator.status(&self.fingerprint, None);

        Ok(HwReport {
            fingerprint: (*self.fingerprint).clone(),
            ffmpeg: probe,
            capabilities,
            capacity,
            recent_failures,
            hailo,
        })
    }

    /// Return live [`TranscodeCapacity`] for any authenticated user (`read` scope).
    pub async fn get_capacity(&self, _actor: &Actor) -> Result<TranscodeCapacity> {
        let ffmpeg_bin = self.resolve_ffmpeg_bin();
        let probe = blocking(move || Ok(probe_ffmpeg(&ffmpeg_bin))).await?;
        let rows = hw::list_all(&self.db).await?;
        let capabilities = rows_to_entries(rows);
        Ok(compute_capacity(
            &self.fingerprint,
            &probe,
            &capabilities,
            &self.tracker,
            self.active_video_sessions(),
        ))
    }

    /// Run the functional 2 s hardware self-test across candidate backends, persist results in
    /// `hw_capabilities`, record a completed `hw.selftest` job in `jobs`, and return [`HwSelftestResponse`].
    /// Requires admin privileges.
    pub async fn run_selftest(&self, actor: &Actor) -> Result<HwSelftestResponse> {
        if !actor.is_admin {
            return Err(Error::Forbidden);
        }

        let jobs_repo = JobsRepo::new(self.db.clone());
        let started_ms = now_ms();
        let job_id = jobs_repo.insert("hw.selftest", "{}", 10, started_ms).await?;

        let ffmpeg_bin = self.resolve_ffmpeg_bin();
        let fp = (*self.fingerprint).clone();
        let (probe, entries) = blocking(move || {
            let probe = probe_ffmpeg(&ffmpeg_bin);
            let entries = run_selftest(&ffmpeg_bin, &fp, &probe);
            Ok((probe, entries))
        })
        .await?;

        let db_rows: Vec<HwCapabilityRow> = entries
            .iter()
            .map(|e| HwCapabilityRow {
                backend: e.backend.clone(),
                codec: e.codec.clone(),
                operation: e.operation.clone(),
                ok: e.ok,
                fps: e.fps,
                error: e.error.clone(),
                ffmpeg_hash: e.ffmpeg_hash.clone(),
                driver_fingerprint: e.driver_fingerprint.clone(),
                tested_at: e.tested_at,
            })
            .collect();

        hw::replace_all(&self.db, &db_rows).await?;
        self.tracker.clear();
        jobs_repo.finish(job_id, DONE, None, now_ms()).await?;

        let capacity = compute_capacity(
            &self.fingerprint,
            &probe,
            &entries,
            &self.tracker,
            self.active_video_sessions(),
        );

        Ok(HwSelftestResponse { job_id, capabilities: entries, capacity })
    }

    /// Conservative admission for a two-level Cast ladder. A recent matching
    /// benchmark is required; platform guesses alone never enable this path.
    pub async fn supports_cast_ladder(&self, backend: &str, frame_rate: f64) -> Result<bool> {
        if self.fingerprint.profile.is_pi_or_arm()
            || !frame_rate.is_finite()
            || frame_rate <= 0.0
            || frame_rate > 30.0
        {
            return Ok(false);
        }
        let ffmpeg_bin = self.resolve_ffmpeg_bin();
        let probe = blocking(move || Ok(probe_ffmpeg(&ffmpeg_bin))).await?;
        let entries = rows_to_entries(hw::list_all(&self.db).await?);
        let capacity = compute_capacity(
            &self.fingerprint,
            &probe,
            &entries,
            &self.tracker,
            self.active_video_sessions(),
        );
        Ok(capacity.supports_abr_ladder
            && capacity.preferred_backend == backend
            && capacity.available_video_sessions >= 2
            && cast_benchmark_matches(
                &entries,
                backend,
                &probe.binary_hash,
                &self.fingerprint.driver_fingerprint,
                frame_rate,
                now_ms(),
            ))
    }

    /// Build `(can_transcode_video, Option<HwSummary>, TranscodeCapacity)` for the playback decision engine.
    pub async fn decision_summary(&self) -> Result<(bool, Option<HwSummary>, TranscodeCapacity)> {
        let ffmpeg_bin = self.resolve_ffmpeg_bin();
        let probe = blocking(move || Ok(probe_ffmpeg(&ffmpeg_bin))).await?;
        let rows = hw::list_all(&self.db).await?;
        let capabilities = rows_to_entries(rows);
        let capacity = compute_capacity(
            &self.fingerprint,
            &probe,
            &capabilities,
            &self.tracker,
            self.active_video_sessions(),
        );

        let hw_summary = if capacity.preferred_backend != "software"
            || self.fingerprint.profile.is_pi_or_arm()
        {
            Some(HwSummary {
                backend: capacity.preferred_backend.clone(),
                encode_codecs: capacity.encode_codecs.clone(),
                can_tonemap: capacity.can_tonemap,
            })
        } else {
            None
        };

        Ok((capacity.can_transcode_video, hw_summary, capacity))
    }
}

fn rows_to_entries(rows: Vec<HwCapabilityRow>) -> Vec<HwCapEntry> {
    rows.into_iter()
        .map(|r| HwCapEntry {
            backend: r.backend,
            codec: r.codec,
            operation: r.operation,
            ok: r.ok,
            fps: r.fps,
            error: r.error,
            ffmpeg_hash: r.ffmpeg_hash,
            driver_fingerprint: r.driver_fingerprint,
            tested_at: r.tested_at,
        })
        .collect()
}

fn cast_benchmark_matches(
    entries: &[HwCapEntry],
    backend: &str,
    hash: &str,
    driver: &str,
    frame_rate: f64,
    now: i64,
) -> bool {
    entries.iter().any(|entry| {
        entry.backend == backend
            && entry.codec == "h264"
            && entry.operation == "encode"
            && entry.ok
            && entry.ffmpeg_hash == hash
            && entry.driver_fingerprint == driver
            && (0..=86_400_000).contains(&now.saturating_sub(entry.tested_at))
            && entry.fps.is_some_and(|fps| fps.is_finite() && fps >= frame_rate * 3.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_ladder_requires_recent_matching_successful_measurement() {
        let mut entry = HwCapEntry {
            backend: "nvenc".into(),
            codec: "h264".into(),
            operation: "encode".into(),
            ok: true,
            fps: Some(100.0),
            error: None,
            ffmpeg_hash: "binary".into(),
            driver_fingerprint: "driver".into(),
            tested_at: 100_000_000,
        };
        let accepted = |entry: &HwCapEntry, now| {
            cast_benchmark_matches(
                std::slice::from_ref(entry),
                "nvenc",
                "binary",
                "driver",
                30.0,
                now,
            )
        };
        assert!(accepted(&entry, 100_000_001));
        assert!(!accepted(&entry, 200_000_000));
        assert!(!accepted(&entry, 99_999_999));
        entry.fps = Some(89.0);
        assert!(!accepted(&entry, 100_000_001));
        entry.fps = Some(f64::NAN);
        assert!(!accepted(&entry, 100_000_001));
        entry.fps = Some(100.0);
        entry.ffmpeg_hash = "changed".into();
        assert!(!accepted(&entry, 100_000_001));
        entry.ffmpeg_hash = "binary".into();
        entry.driver_fingerprint = "changed".into();
        assert!(!accepted(&entry, 100_000_001));
        entry.driver_fingerprint = "driver".into();
        entry.ok = false;
        assert!(!accepted(&entry, 100_000_001));
    }
}
