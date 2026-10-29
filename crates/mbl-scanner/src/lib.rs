//! Library filesystem walking, change detection and watching.
//!
//! A scan walks the library roots, compares files with known parts (path, size, mtime), detects
//! moves by quick hash, probes new/changed files with bounded concurrency, writes items in batched
//! transactions, and marks vanished files as missing (never deleting them).

mod hash;
pub mod subtitles;
mod walk;
pub mod watch;
mod write;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use mbl_core::events::{Event, EventBus};
use mbl_core::{Error, ItemId, LibraryId, PartId, Result, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::media::{self, PartFile};
use mbl_jobs::{JobCtx, JobHandler, JobQueue, Priority};
use mbl_probe::{DEFAULT_PROBE_TIMEOUT, FfTools, MediaInfo};
use serde_json::json;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub use walk::{AUDIO_EXTS, FoundFile, VIDEO_EXTS};
pub use watch::{LibraryWatcher, WatchConfig};
pub use write::sort_title;

pub const SCAN_JOB: &str = mbl_jobs::kinds::LIBRARY_SCAN;
pub const MATCH_JOB: &str = mbl_jobs::kinds::METADATA_MATCH;
/// Files written per transaction.
const BATCH: usize = 50;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub files_seen: usize,
    pub unchanged: usize,
    pub probed: usize,
    pub added: usize,
    pub changed: usize,
    pub moved: usize,
    pub missing: usize,
    pub failed: usize,
    pub new_items: usize,
    pub unavailable_roots: Vec<String>,
}

#[derive(Clone)]
pub struct Scanner {
    db: Db,
    tools: Option<FfTools>,
    events: EventBus,
    jobs: Option<JobQueue>,
    probe_concurrency: usize,
}

struct Probed {
    file: FoundFile,
    existing: Option<PartId>,
    hash: Option<Vec<u8>>,
    info: Result<MediaInfo>,
}

impl Scanner {
    pub fn new(
        db: Db,
        tools: Option<FfTools>,
        events: EventBus,
        jobs: Option<JobQueue>,
        probe_concurrency: usize,
    ) -> Self {
        Self { db, tools, events, jobs, probe_concurrency: probe_concurrency.max(1) }
    }

    /// Scan one library. `on_progress` receives `0.0..=1.0`.
    pub async fn scan(
        &self,
        library_id: LibraryId,
        cancel: &CancellationToken,
        on_progress: impl Fn(f32) + Send + Sync,
    ) -> Result<ScanStats> {
        self.scan_folders(library_id, &[], cancel, on_progress).await
    }

    /// Scan only `folders` (the whole library when empty). Folders outside the library's roots
    /// are ignored; known files outside them are left untouched.
    pub async fn scan_folders(
        &self,
        library_id: LibraryId,
        folders: &[String],
        cancel: &CancellationToken,
        on_progress: impl Fn(f32) + Send + Sync,
    ) -> Result<ScanStats> {
        let started = std::time::Instant::now();
        let library =
            LibrariesRepo::new(self.db.clone()).get(library_id).await?.ok_or(Error::NotFound)?;
        // DVR publishes only finalized, probed files atomically. A filesystem scan could
        // otherwise import an active/failed recording and bypass that publication gate.
        if mbl_db::repo::dvr::is_library(&self.db, library_id).await? {
            on_progress(1.0);
            return Ok(ScanStats::default());
        }
        let tools = self
            .tools
            .clone()
            .ok_or_else(|| Error::Unavailable("ffprobe is not available; install FFmpeg".into()))?;
        let mut stats = ScanStats::default();

        let known: HashMap<String, PartFile> = media::parts_for_library(&self.db, library_id)
            .await?
            .into_iter()
            .map(|p| (p.path.clone(), p))
            .collect();

        let sep = std::path::MAIN_SEPARATOR;
        let in_scope = |path: &str| {
            folders.is_empty() || folders.iter().any(|f| path.starts_with(&format!("{f}{sep}")))
        };

        // Walk available roots (blocking I/O off the executor).
        let mut found = Vec::new();
        let mut unavailable = Vec::new();
        for root in &library.paths {
            let prefix = format!("{root}{sep}");
            let starts: Vec<String> = if folders.is_empty() {
                vec![root.clone()]
            } else {
                folders.iter().filter(|f| *f == root || f.starts_with(&prefix)).cloned().collect()
            };
            if starts.is_empty() {
                continue;
            }
            let has_known = known.keys().any(|p| p.starts_with(&prefix));
            let (root_c, kind) = (root.clone(), library.kind);
            let files = tokio::task::spawn_blocking(move || {
                walk::root_available(&root_c, has_known).then(|| {
                    starts
                        .iter()
                        .flat_map(|s| walk::walk_from(&root_c, s, kind))
                        .collect::<Vec<_>>()
                })
            })
            .await
            .map_err(Error::internal)?;
            match files {
                Some(f) => found.extend(f),
                None => {
                    tracing::warn!(library_id = %library_id, root, "library root unavailable; not marking files missing");
                    self.events
                        .publish(Event::LibraryUnavailable { library_id, path: root.clone() });
                    unavailable.push(prefix);
                    stats.unavailable_roots.push(root.clone());
                }
            }
        }
        stats.files_seen = found.len();

        // Classify against known parts.
        let mut seen: HashSet<&str> = HashSet::new();
        let mut to_probe: Vec<(FoundFile, Option<PartId>)> = Vec::new();
        let mut new_files = Vec::new();
        let mut reappeared = Vec::new();
        for file in found {
            match known.get(&file.path) {
                Some(part) if part.size == file.size && part.mtime == file.mtime => {
                    seen.insert(part.path.as_str());
                    stats.unchanged += 1;
                    if part.missing_since.is_some() {
                        reappeared.push(part.id);
                    }
                }
                Some(part) => {
                    seen.insert(part.path.as_str());
                    to_probe.push((file, Some(part.id)));
                }
                None => new_files.push(file),
            }
        }
        let vanished: Vec<&PartFile> = known
            .values()
            .filter(|p| !seen.contains(p.path.as_str()) && in_scope(&p.path))
            .filter(|p| !unavailable.iter().any(|root| p.path.starts_with(root)))
            .collect();

        // Hash new files; a match with a vanished part of the same size is a move.
        let hashes = self.hash_all(&new_files).await;
        let mut moves: Vec<(PartId, FoundFile)> = Vec::new();
        let mut moved_ids: HashSet<PartId> = HashSet::new();
        let mut new_with_hash = Vec::new();
        for (file, hash) in new_files.into_iter().zip(hashes) {
            let target = hash.as_ref().and_then(|h| {
                vanished.iter().find(|p| {
                    !moved_ids.contains(&p.id)
                        && p.size == file.size
                        && p.quick_hash.as_deref() == Some(h.as_slice())
                })
            });
            match target {
                Some(p) => {
                    moved_ids.insert(p.id);
                    moves.push((p.id, file));
                }
                None => new_with_hash.push((file, hash)),
            }
        }

        let now = now_ms();
        {
            let mut tx = self.db.write.begin().await?;
            for id in &reappeared {
                media::clear_missing(&mut tx, *id).await?;
            }
            for (id, file) in &moves {
                tracing::info!(part_id = %id, to = %file.path, "file moved");
                media::move_part(&mut tx, *id, &file.path, file.mtime).await?;
            }
            for part in
                vanished.iter().filter(|p| !moved_ids.contains(&p.id) && p.missing_since.is_none())
            {
                media::set_missing(&mut tx, part.id, now).await?;
                stats.missing += 1;
            }
            tx.commit().await?;
        }
        stats.moved = moves.len();

        // Probe changed + new files in chunks; write each chunk in one transaction.
        let mut work: Vec<(FoundFile, Option<PartId>, Option<Vec<u8>>)> =
            to_probe.into_iter().map(|(f, id)| (f, id, None)).collect();
        work.extend(new_with_hash.into_iter().map(|(f, h)| (f, None, h)));
        let total = work.len();
        let mut new_top_items: Vec<ItemId> = Vec::new();
        let mut done = 0usize;
        on_progress(if total == 0 { 1.0 } else { 0.0 });

        let mut iter = work.into_iter().peekable();
        while iter.peek().is_some() {
            if cancel.is_cancelled() {
                tracing::info!(library_id = %library_id, "scan cancelled");
                break;
            }
            let chunk: Vec<_> = iter.by_ref().take(BATCH).collect();
            let probed = self.probe_all(&tools, chunk).await;
            stats.probed += probed.len();
            let mut tx = self.db.write.begin().await?;
            for p in probed {
                match self.write_one(&mut tx, library_id, library.kind, &p, now).await {
                    Ok(Some(items)) => {
                        stats.added += 1;
                        new_top_items.extend(items);
                    }
                    Ok(None) => stats.changed += 1,
                    Err(e) => {
                        stats.failed += 1;
                        tracing::warn!(path = %p.file.path, error = %e, "could not import file");
                    }
                }
            }
            tx.commit().await?;
            done += BATCH.min(total - done);
            on_progress(done as f32 / total.max(1) as f32);
            self.events.publish(Event::ScanProgress {
                library_id,
                scanned: done as u64,
                total: Some(total as u64),
                done: false,
            });
        }

        stats.new_items = new_top_items.len();
        if let Some(jobs) = &self.jobs {
            for id in &new_top_items {
                jobs.enqueue(MATCH_JOB, &json!({ "itemId": id }), Priority::Normal).await?;
            }
        }
        if total > 0
            && matches!(
                library.kind,
                mbl_core::LibraryKind::Movie
                    | mbl_core::LibraryKind::Show
                    | mbl_core::LibraryKind::HomeVideo
            )
            && let Some(jobs) = &self.jobs
        {
            jobs.enqueue(
                mbl_jobs::kinds::MEDIA_TRICKPLAY,
                &json!({"libraryId": library_id}),
                Priority::Idle,
            )
            .await?;
        }
        media::touch_library_paths(&self.db, library_id, now).await?;
        self.events.publish(Event::ScanProgress {
            library_id,
            scanned: total as u64,
            total: Some(total as u64),
            done: true,
        });
        self.events.publish(Event::LibraryUpdated { library_id });
        tracing::info!(library_id = %library_id, elapsed = ?started.elapsed(), ?stats, "scan finished");
        Ok(stats)
    }

    /// Returns `Some(new top-level items)` for an added file, `None` for an updated one.
    async fn write_one(
        &self,
        conn: &mut sqlx::SqliteConnection,
        library_id: LibraryId,
        kind: mbl_core::LibraryKind,
        p: &Probed,
        now: i64,
    ) -> Result<Option<Vec<ItemId>>> {
        let info = p.info.as_ref().map_err(|e| Error::Validation(e.to_string()))?;
        if let Some(part_id) = p.existing {
            let probe = serde_json::to_string(info).map_err(Error::internal)?;
            let chapters = serde_json::to_string(&info.chapters).map_err(Error::internal)?;
            let duration = i64::try_from(info.duration_ms).ok();
            let version = media::update_part_probe(
                conn,
                part_id,
                &media::PartUpdate {
                    size: p.file.size,
                    mtime: p.file.mtime,
                    quick_hash: p.hash.as_deref(),
                    duration_ms: duration,
                    probe: &probe,
                    chapters: &chapters,
                },
            )
            .await?;
            media::replace_streams(conn, part_id, &write::streams(info)).await?;
            let summary = write::Summary::new(info);
            media::update_version_summary(conn, version, &summary.as_version(None)).await?;
            return Ok(None);
        }
        let ctx = write::FileCtx {
            library_id,
            kind,
            file: &p.file,
            info,
            quick_hash: p.hash.as_deref(),
            now,
        };
        Ok(Some(write::write_new(conn, &ctx).await?.new_top_items))
    }

    async fn hash_all(&self, files: &[FoundFile]) -> Vec<Option<Vec<u8>>> {
        let sem = Arc::new(Semaphore::new(self.probe_concurrency));
        let mut set = JoinSet::new();
        for (i, f) in files.iter().enumerate() {
            let (sem, path) = (sem.clone(), f.path.clone());
            set.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let hash = tokio::task::spawn_blocking(move || hash::quick_hash(&path).ok())
                    .await
                    .ok()
                    .flatten();
                (i, hash)
            });
        }
        let mut out = vec![None; files.len()];
        while let Some(res) = set.join_next().await {
            if let Ok((i, h)) = res {
                out[i] = h;
            }
        }
        out
    }

    async fn probe_all(
        &self,
        tools: &FfTools,
        chunk: Vec<(FoundFile, Option<PartId>, Option<Vec<u8>>)>,
    ) -> Vec<Probed> {
        let sem = Arc::new(Semaphore::new(self.probe_concurrency));
        let mut set = JoinSet::new();
        for (i, (file, existing, hash)) in chunk.into_iter().enumerate() {
            let (sem, tools) = (sem.clone(), tools.clone());
            set.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let hash = match hash {
                    Some(h) => Some(h),
                    None => {
                        let path = file.path.clone();
                        tokio::task::spawn_blocking(move || hash::quick_hash(&path).ok())
                            .await
                            .ok()
                            .flatten()
                    }
                };
                let mut info =
                    tools.probe(std::path::Path::new(&file.path), DEFAULT_PROBE_TIMEOUT).await;
                if let Ok(media_info) = &mut info {
                    let next_idx =
                        media_info.streams.iter().map(|s| s.index + 1).max().unwrap_or(0);
                    let sidecars =
                        subtitles::discover_sidecars(std::path::Path::new(&file.path), next_idx);
                    media_info.streams.extend(sidecars);
                }
                (i, Probed { file, existing, hash, info })
            });
        }
        let mut out: Vec<(usize, Probed)> = Vec::new();
        while let Some(res) = set.join_next().await {
            if let Ok(p) = res {
                out.push(p);
            }
        }
        // Deterministic write order (e.g. cd1 before cd2).
        out.sort_by_key(|(i, _)| *i);
        out.into_iter().map(|(_, p)| p).collect()
    }
}

/// `library.scan` job: payload `{"libraryId": n}`, optionally with `"folders": [..]` for a
/// partial scan.
pub struct ScanHandler {
    pub scanner: Scanner,
}

#[async_trait]
impl JobHandler for ScanHandler {
    fn kind(&self) -> &'static str {
        SCAN_JOB
    }

    async fn run(&self, ctx: JobCtx, payload: serde_json::Value) -> Result<()> {
        let library_id = payload["libraryId"]
            .as_i64()
            .map(LibraryId)
            .ok_or_else(|| Error::Validation("scan job without libraryId".into()))?;
        let folders: Vec<String> = payload["folders"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        let progress = ctx.progress.clone();
        let last = std::sync::Mutex::new(-1i32);
        let handle = tokio::runtime::Handle::current();
        self.scanner
            .scan_folders(library_id, &folders, &ctx.cancel, move |p| {
                let pct = (p * 100.0) as i32;
                let mut last = last.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if pct > *last {
                    *last = pct;
                    let progress = progress.clone();
                    handle.spawn(async move {
                        let _ = progress.report(p).await;
                    });
                }
            })
            .await
            .map(|_| ())
    }
}
