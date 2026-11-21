//! Scheduled database upkeep and cleanup.
//!
//! Every run optimizes the database and folds the write-ahead log back in, vacuums when much of
//! the file is unused, keeps a number of automatic backups and removes old finished jobs.
//! Removing titles whose files are gone happens only when asked (with a preview first), or
//! automatically after a chosen number of days, and never in a library whose folders cannot be
//! read right now (an unplugged disk is not a deleted file) or on a storage node.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, LibraryId, Result};
use mbl_db::Db;
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::maintenance as repo;
use mbl_db::repo::settings::SettingsRepo;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const DAY_MS: i64 = 86_400_000;
/// Finished jobs are kept this long.
const JOB_KEEP_MS: i64 = 30 * DAY_MS;
/// Vacuum when at least this share of the file is unused.
const VACUUM_SHARE: f64 = 0.2;
/// Titles gone longer than this are suggested for removal in a manual run when no automatic
/// limit is set.
const DEFAULT_GONE_DAYS: i64 = 30;
const EXAMPLES: usize = 10;
const AUTO_BACKUP_PREFIX: &str = "mbl-auto-";

/// What a run did, or would do in a preview.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceReport {
    pub dry_run: bool,
    pub started_at: i64,
    pub duration_ms: i64,
    pub database_bytes: i64,
    /// Unused space in the database file before the run.
    pub unused_bytes: i64,
    pub vacuumed: bool,
    /// File name of the backup made (or that would be made).
    pub backup: Option<String>,
    pub backups_removed: u32,
    /// Finished jobs older than 30 days.
    pub old_jobs: i64,
    /// Titles whose files have all been missing longer than `goneDays`.
    pub gone_items: i64,
    /// Seasons, shows, albums and artists with nothing left in them.
    pub empty_items: i64,
    pub gone_days: i64,
    /// Up to ten of the titles that are (or would be) removed.
    pub examples: Vec<String>,
    /// Whether titles were removed in this run.
    pub cleaned: bool,
    /// Libraries left alone because their folders could not be read.
    pub unreachable_libraries: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceSettings {
    pub enabled: bool,
    /// Local hour (0–23) to run at.
    pub hour: u32,
    pub interval_days: u32,
    /// Automatic backups to keep; 0 makes none.
    pub keep_backups: u32,
    /// Remove titles whose files have been gone this many days; 0 never removes automatically.
    pub cleanup_after_days: u32,
    /// The viewer's offset from UTC in minutes, so `hour` means their local time.
    pub utc_offset_minutes: i32,
    pub last_run: Option<i64>,
    pub last_report: Option<MaintenanceReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMaintenanceSettings {
    pub enabled: bool,
    pub hour: u32,
    pub interval_days: u32,
    pub keep_backups: u32,
    pub cleanup_after_days: u32,
    #[serde(default)]
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunMaintenance {
    /// Report what would happen without changing anything.
    #[serde(default)]
    pub dry_run: bool,
    /// Also remove titles whose files are gone (and empty seasons/shows).
    #[serde(default)]
    pub cleanup: bool,
}

#[derive(Debug, Clone)]
pub struct MaintenanceService {
    db: Db,
    running: Arc<tokio::sync::Mutex<()>>,
}

fn settings_key(name: &str) -> String {
    format!("maintenance_{name}")
}

/// `<dir of the database>/backups`.
pub async fn backup_dir(db: &Db) -> Result<PathBuf> {
    let file = repo::db_file(db)
        .await?
        .ok_or_else(|| Error::Validation("The database is not a file".into()))?;
    Ok(Path::new(&file).parent().unwrap_or(Path::new(".")).join("backups"))
}

/// Automatic backups, newest first.
fn auto_backups(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(AUTO_BACKUP_PREFIX) && n.ends_with(".db"))
        })
        .collect();
    // The name holds the time, so sorting by name sorts by age.
    found.sort();
    found.reverse();
    found
}

/// Every folder of a library can be read now and has something in it: an unplugged disk often
/// leaves an empty mount point behind. A node library never counts.
fn reachable(paths: &[String]) -> bool {
    !paths.is_empty()
        && paths.iter().all(|p| {
            !p.starts_with(mbl_core::NODE_URI_SCHEME)
                && std::fs::read_dir(p).is_ok_and(|mut d| d.next().is_some())
        })
}

/// Whether a scheduled run is due: enabled, not run within the interval, and the local hour has
/// come (or the last run is so old that it should not wait any longer).
pub fn due(settings: &MaintenanceSettings, now: i64) -> bool {
    if !settings.enabled {
        return false;
    }
    let interval = i64::from(settings.interval_days.max(1)) * DAY_MS;
    let Some(last) = settings.last_run else {
        return local_hour(now, settings.utc_offset_minutes) == settings.hour;
    };
    let since = now - last;
    // A little slack so a daily run at 04:00 is not pushed to 04:10 the next day.
    if since < interval - 3_600_000 {
        return false;
    }
    since >= interval + DAY_MS || local_hour(now, settings.utc_offset_minutes) == settings.hour
}

fn local_hour(now: i64, offset_minutes: i32) -> u32 {
    let minutes = now.div_euclid(60_000) + i64::from(offset_minutes);
    u32::try_from(minutes.div_euclid(60).rem_euclid(24)).unwrap_or(0)
}

impl MaintenanceService {
    pub fn new(db: Db) -> Self {
        Self { db, running: Arc::default() }
    }

    fn store(&self) -> SettingsRepo {
        SettingsRepo::new(self.db.clone())
    }

    async fn number(&self, name: &str, default: i64) -> Result<i64> {
        Ok(self
            .store()
            .get(&settings_key(name))
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(default))
    }

    async fn current(&self) -> Result<MaintenanceSettings> {
        let store = self.store();
        let report = store
            .get(&settings_key("last_report"))
            .await?
            .and_then(|r| serde_json::from_str(&r).ok());
        Ok(MaintenanceSettings {
            enabled: store.get(&settings_key("enabled")).await?.as_deref() != Some("false"),
            hour: u32::try_from(self.number("hour", 4).await?).unwrap_or(4),
            interval_days: u32::try_from(self.number("interval_days", 1).await?).unwrap_or(1),
            keep_backups: u32::try_from(self.number("keep_backups", 7).await?).unwrap_or(7),
            cleanup_after_days: u32::try_from(self.number("cleanup_after_days", 0).await?)
                .unwrap_or(0),
            utc_offset_minutes: i32::try_from(self.number("utc_offset_minutes", 0).await?)
                .unwrap_or(0),
            last_run: store.get(&settings_key("last_run")).await?.and_then(|v| v.parse().ok()),
            last_report: report,
        })
    }

    pub async fn get_settings(&self, actor: &Actor) -> Result<MaintenanceSettings> {
        actor.require_admin()?;
        self.current().await
    }

    pub async fn update_settings(
        &self,
        actor: &Actor,
        update: UpdateMaintenanceSettings,
    ) -> Result<MaintenanceSettings> {
        actor.require_admin()?;
        if update.hour > 23
            || !(1..=30).contains(&update.interval_days)
            || update.keep_backups > 60
            || (update.cleanup_after_days != 0 && !(7..=365).contains(&update.cleanup_after_days))
            || update.utc_offset_minutes.abs() > 14 * 60
        {
            return Err(Error::Validation(
                "Choose an hour 0–23, every 1–30 days, at most 60 backups and cleanup after \
                 7–365 days (or never)"
                    .into(),
            ));
        }
        let entries = [
            (settings_key("enabled"), update.enabled.to_string()),
            (settings_key("hour"), update.hour.to_string()),
            (settings_key("interval_days"), update.interval_days.to_string()),
            (settings_key("keep_backups"), update.keep_backups.to_string()),
            (settings_key("cleanup_after_days"), update.cleanup_after_days.to_string()),
            (settings_key("utc_offset_minutes"), update.utc_offset_minutes.to_string()),
        ];
        let refs: Vec<(&str, &str)> =
            entries.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.store().set_many(&refs).await?;
        self.current().await
    }

    /// Run now (admin). A preview changes nothing.
    pub async fn run(&self, actor: &Actor, req: RunMaintenance) -> Result<MaintenanceReport> {
        actor.require_admin()?;
        let settings = self.current().await?;
        let days = match settings.cleanup_after_days {
            0 => DEFAULT_GONE_DAYS,
            d => i64::from(d),
        };
        self.execute(&settings, req.dry_run, req.cleanup.then_some(days)).await
    }

    /// Run when the schedule says so (called every few minutes by the server).
    pub async fn run_if_due(&self) -> Result<Option<MaintenanceReport>> {
        let settings = self.current().await?;
        if !due(&settings, now_ms()) {
            return Ok(None);
        }
        let cleanup =
            (settings.cleanup_after_days > 0).then(|| i64::from(settings.cleanup_after_days));
        self.execute(&settings, false, cleanup).await.map(Some)
    }

    async fn execute(
        &self,
        settings: &MaintenanceSettings,
        dry_run: bool,
        cleanup_days: Option<i64>,
    ) -> Result<MaintenanceReport> {
        let Ok(_guard) = self.running.try_lock() else {
            return Err(Error::Conflict("maintenance is already running".into()));
        };
        let started = now_ms();
        let mut report = MaintenanceReport { dry_run, started_at: started, ..Default::default() };
        let (bytes, unused) = repo::size(&self.db).await?;
        report.database_bytes = bytes;
        report.unused_bytes = unused;

        // Titles whose files are gone, only in libraries whose folders can be read.
        let days = cleanup_days.unwrap_or(match settings.cleanup_after_days {
            0 => DEFAULT_GONE_DAYS,
            d => i64::from(d),
        });
        report.gone_days = days;
        let mut readable: Vec<LibraryId> = Vec::new();
        for lib in LibrariesRepo::new(self.db.clone()).list().await? {
            let paths = lib.paths.clone();
            let ok = tokio::task::spawn_blocking(move || reachable(&paths)).await.unwrap_or(false);
            if ok {
                readable.push(lib.id);
            } else {
                report.unreachable_libraries.push(lib.name);
            }
        }
        let gone = repo::gone_items(&self.db, &readable, started - days * DAY_MS).await?;
        report.gone_items = gone.len() as i64;
        report.examples = gone.iter().take(EXAMPLES).map(|g| g.title.clone()).collect();
        let apply_cleanup = cleanup_days.is_some() && !dry_run;
        if apply_cleanup && !gone.is_empty() {
            let ids: Vec<i64> = gone.iter().map(|g| g.id).collect();
            repo::delete_items(&self.db, &ids).await?;
            report.cleaned = true;
        }
        // Empty containers: counted always, removed with a cleanup (repeat for show → season).
        let mut empty = repo::empty_containers(&self.db, &readable).await?;
        report.empty_items = empty.len() as i64;
        if apply_cleanup {
            let mut total = 0;
            while !empty.is_empty() && total < 10_000 {
                let ids: Vec<i64> = empty.iter().map(|g| g.id).collect();
                total += repo::delete_items(&self.db, &ids).await? as i64;
                empty = repo::empty_containers(&self.db, &readable).await?;
            }
            report.empty_items = total;
            report.cleaned |= total > 0;
        }

        report.old_jobs = repo::old_jobs(&self.db, started - JOB_KEEP_MS, !dry_run).await?;

        if !dry_run {
            repo::optimize(&self.db).await?;
            let share = if bytes > 0 { unused as f64 / bytes as f64 } else { 0.0 };
            if share >= VACUUM_SHARE || report.cleaned {
                repo::vacuum(&self.db).await?;
                report.vacuumed = true;
            }
        }

        if settings.keep_backups > 0 {
            let dir = backup_dir(&self.db).await?;
            let name = format!("{AUTO_BACKUP_PREFIX}{started}.db");
            report.backup = Some(name.clone());
            if !dry_run {
                tokio::fs::create_dir_all(&dir).await.map_err(Error::internal)?;
                let target = dir.join(&name);
                let target =
                    target.to_str().ok_or_else(|| Error::Validation("Bad backup path".into()))?;
                repo::backup_into(&self.db, target).await?;
                for old in auto_backups(&dir).into_iter().skip(settings.keep_backups as usize) {
                    if tokio::fs::remove_file(&old).await.is_ok() {
                        report.backups_removed += 1;
                    }
                }
            }
        }

        report.duration_ms = now_ms() - started;
        if !dry_run {
            let json = serde_json::to_string(&report).unwrap_or_default();
            let entries = [
                (settings_key("last_run"), started.to_string()),
                (settings_key("last_report"), json),
            ];
            let refs: Vec<(&str, &str)> =
                entries.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            self.store().set_many(&refs).await?;
            tracing::info!(
                cleaned = report.cleaned,
                gone = report.gone_items,
                vacuumed = report.vacuumed,
                "database maintenance finished"
            );
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(last_run: Option<i64>) -> MaintenanceSettings {
        MaintenanceSettings {
            enabled: true,
            hour: 4,
            interval_days: 1,
            keep_backups: 7,
            cleanup_after_days: 0,
            utc_offset_minutes: 60,
            last_run,
            last_report: None,
        }
    }

    #[test]
    fn runs_at_the_local_hour_once_per_interval() {
        let at = |h: i64| 20_000 * DAY_MS + h * 3_600_000; // UTC hour h
        // 03:00 UTC is 04:00 at UTC+1.
        assert!(due(&settings(None), at(3)));
        assert!(!due(&settings(None), at(4)));
        // Ran yesterday at the same time: due again; ran an hour ago: not.
        assert!(due(&settings(Some(at(3) - DAY_MS)), at(3)));
        assert!(!due(&settings(Some(at(2))), at(3)));
        // Missed for more than a day: runs whatever the hour.
        assert!(due(&settings(Some(at(10) - 3 * DAY_MS)), at(10)));
        let mut off = settings(None);
        off.enabled = false;
        assert!(!due(&off, at(3)));
    }

    #[test]
    fn unreadable_or_node_folders_are_not_reachable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!reachable(&[dir.path().to_str().unwrap().to_owned()]), "empty folder");
        std::fs::write(dir.path().join("a.mkv"), "x").unwrap();
        assert!(reachable(&[dir.path().to_str().unwrap().to_owned()]));
        assert!(!reachable(&["/does/not/exist".into()]));
        assert!(!reachable(&["mblnode://n/1".into()]));
        assert!(!reachable(&[]));
    }
}
