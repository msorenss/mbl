//! Filesystem watcher: queues partial scans for folders that changed.
//!
//! Local roots use OS notifications (`notify`). Events are debounced, and a file only counts
//! as settled when its size is unchanged across two checks, so copies in progress aren't
//! probed half-written. Network roots are polled because notifications are unreliable there.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mbl_core::LibraryId;
use mbl_db::Db;
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_jobs::{JobQueue, Priority};
use notify::{EventKind, RecursiveMode, Watcher};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::SCAN_JOB;

#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub debounce: Duration,
    pub settle: Duration,
    pub poll_interval: Duration,
    /// How often library roots are re-read to pick up added/removed libraries.
    pub reload_interval: Duration,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_secs(5),
            settle: Duration::from_secs(3),
            poll_interval: Duration::from_secs(15 * 60),
            reload_interval: Duration::from_secs(30),
        }
    }
}

pub struct LibraryWatcher {
    db: Db,
    jobs: JobQueue,
    cfg: WatchConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Root {
    library: LibraryId,
    path: String,
    network: bool,
}

struct Pending {
    library: LibraryId,
    folder: String,
    last_event: Instant,
}

impl LibraryWatcher {
    pub fn new(db: Db, jobs: JobQueue, cfg: WatchConfig) -> Self {
        Self { db, jobs, cfg }
    }

    pub fn spawn(self, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move { self.run(stop).await })
    }

    async fn run(self, stop: CancellationToken) {
        let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();
        let watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
                Ok(ev) if !matches!(ev.kind, EventKind::Access(_)) => {
                    for p in ev.paths {
                        let _ = tx.send(p);
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "filesystem watcher error"),
            });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "filesystem watching unavailable; rescan manually");
                return;
            }
        };

        let mut roots: Vec<Root> = Vec::new();
        let mut last_poll: HashMap<String, Instant> = HashMap::new();
        let mut pending: HashMap<PathBuf, Pending> = HashMap::new();
        let mut last_reload: Option<Instant> = None;
        let mut tick = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                Some(path) = rx.recv() => {
                    if let Some(root) = root_for(&roots, &path) {
                        let folder = folder_for(&path, &root.path);
                        pending.insert(path, Pending { library: root.library, folder, last_event: Instant::now() });
                    }
                }
                _ = tick.tick() => {
                    if last_reload.is_none_or(|t| t.elapsed() >= self.cfg.reload_interval) {
                        last_reload = Some(Instant::now());
                        match self.load_roots().await {
                            Ok(new) if new != roots => {
                                apply_roots(&mut watcher, &roots, &new);
                                roots = new;
                            }
                            Ok(_) => {}
                            Err(e) => tracing::warn!(error = %e, "watcher could not load libraries"),
                        }
                    }
                    self.poll_network(&roots, &mut last_poll).await;
                    self.flush(&mut pending, &stop).await;
                }
            }
        }
    }

    async fn load_roots(&self) -> mbl_core::Result<Vec<Root>> {
        let mut roots = Vec::new();
        for lib in LibrariesRepo::new(self.db.clone()).list().await? {
            for path in lib.paths {
                let p = path.clone();
                let network = tokio::task::spawn_blocking(move || {
                    mbl_platform::netfs::is_network_path(Path::new(&p))
                })
                .await
                .unwrap_or(false);
                roots.push(Root { library: lib.id, path, network });
            }
        }
        Ok(roots)
    }

    async fn poll_network(&self, roots: &[Root], last_poll: &mut HashMap<String, Instant>) {
        for root in roots.iter().filter(|r| r.network) {
            let due =
                last_poll.get(&root.path).is_none_or(|t| t.elapsed() >= self.cfg.poll_interval);
            if due {
                last_poll.insert(root.path.clone(), Instant::now());
                self.enqueue(root.library, BTreeSet::from([root.path.clone()])).await;
            }
        }
    }

    /// Queue scans for paths whose events are older than the debounce and whose size stayed the
    /// same over the settle period; unsettled paths are re-armed.
    async fn flush(&self, pending: &mut HashMap<PathBuf, Pending>, stop: &CancellationToken) {
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, p)| p.last_event.elapsed() >= self.cfg.debounce)
            .map(|(path, _)| path.clone())
            .collect();
        if ready.is_empty() {
            return;
        }
        let before: Vec<Option<u64>> = ready.iter().map(|p| file_size(p)).collect();
        tokio::select! {
            () = stop.cancelled() => return,
            () = tokio::time::sleep(self.cfg.settle) => {}
        }
        let mut by_library: BTreeMap<LibraryId, BTreeSet<String>> = BTreeMap::new();
        for (path, size) in ready.into_iter().zip(before) {
            if file_size(&path) != size {
                if let Some(p) = pending.get_mut(&path) {
                    p.last_event = Instant::now();
                }
                continue;
            }
            if let Some(p) = pending.remove(&path) {
                by_library.entry(p.library).or_default().insert(p.folder);
            }
        }
        for (library, folders) in by_library {
            self.enqueue(library, folders).await;
        }
    }

    async fn enqueue(&self, library: LibraryId, folders: BTreeSet<String>) {
        tracing::debug!(library_id = %library, ?folders, "queueing partial scan");
        let payload = json!({ "libraryId": library, "folders": folders });
        if let Err(e) = self.jobs.enqueue(SCAN_JOB, &payload, Priority::Normal).await {
            tracing::warn!(error = %e, "could not queue scan");
        }
    }
}

fn apply_roots(watcher: &mut impl Watcher, old: &[Root], new: &[Root]) {
    for root in old.iter().filter(|r| !r.network) {
        let _ = watcher.unwatch(Path::new(&root.path));
    }
    for root in new.iter().filter(|r| !r.network) {
        match watcher.watch(Path::new(&root.path), RecursiveMode::Recursive) {
            Ok(()) => tracing::info!(root = %root.path, "watching library root"),
            Err(e) => tracing::warn!(root = %root.path, error = %e, "cannot watch library root"),
        }
    }
    for root in new.iter().filter(|r| r.network) {
        tracing::info!(root = %root.path, "network library root: polling instead of watching");
    }
}

/// The deepest root containing `path`.
fn root_for<'a>(roots: &'a [Root], path: &Path) -> Option<&'a Root> {
    roots.iter().filter(|r| path.starts_with(&r.path)).max_by_key(|r| r.path.len())
}

/// The folder to rescan for a changed path: the path itself for directories, else its parent,
/// never above the root.
fn folder_for(path: &Path, root: &str) -> String {
    let folder = if path.is_dir() { Some(path) } else { path.parent() };
    folder.filter(|f| f.starts_with(root)).and_then(Path::to_str).unwrap_or(root).to_owned()
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_stays_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let movie = dir.path().join("Heat (1995)");
        std::fs::create_dir(&movie).unwrap();
        assert_eq!(folder_for(&movie, root), movie.to_str().unwrap());
        assert_eq!(folder_for(&movie.join("Heat.mkv"), root), movie.to_str().unwrap());
        assert_eq!(folder_for(&dir.path().join("gone.mkv"), root), root);
        assert_eq!(folder_for(dir.path(), root), root);
    }

    #[test]
    fn picks_deepest_root() {
        let roots = vec![
            Root { library: LibraryId(1), path: "/media".into(), network: false },
            Root { library: LibraryId(2), path: "/media/tv".into(), network: false },
        ];
        assert_eq!(root_for(&roots, Path::new("/media/tv/x.mkv")).unwrap().library, LibraryId(2));
        assert_eq!(root_for(&roots, Path::new("/media/m.mkv")).unwrap().library, LibraryId(1));
        assert!(root_for(&roots, Path::new("/mediax/m.mkv")).is_none());
    }
}
