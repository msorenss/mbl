use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use mbl_core::{Error, Result};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct DbOptions {
    /// SQLite page cache per connection, in KiB.
    pub cache_kib: u32,
    pub read_connections: u32,
}

impl Default for DbOptions {
    fn default() -> Self {
        Self { cache_kib: 16 * 1024, read_connections: 4 }
    }
}

impl DbOptions {
    /// Scale the page cache with system RAM: RAM/256, clamped to 8–64 MiB.
    pub fn for_memory(total_bytes: Option<u64>, cpus: usize) -> Self {
        let cache_kib = total_bytes
            .map(|b| (b / 256 / 1024).clamp(8 * 1024, 64 * 1024) as u32)
            .unwrap_or(16 * 1024);
        let read_connections = cpus.clamp(2, 8) as u32;
        Self { cache_kib, read_connections }
    }
}

/// Database handle with separate read and write pools (single-writer pattern).
#[derive(Debug, Clone)]
pub struct Db {
    pub read: SqlitePool,
    pub write: SqlitePool,
}

impl Db {
    /// Open (or create) the database, back it up if migrations are pending, and migrate.
    pub async fn connect(path: &Utf8Path, opts: &DbOptions) -> Result<Self> {
        let existed = std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false);

        let write = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                base_options(path, opts)
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal),
            )
            .await?;

        if existed && has_pending_migrations(&write).await? {
            backup_before_migrate(&write, path).await?;
        }
        MIGRATOR.run(&write).await.map_err(Error::internal)?;

        let read = SqlitePoolOptions::new()
            .max_connections(opts.read_connections)
            .connect_with(base_options(path, opts).read_only(true))
            .await?;

        restrict_to_owner(path.as_std_path());
        tracing::info!(%path, "database ready");
        Ok(Self { read, write })
    }

    pub async fn close(&self) {
        self.read.close().await;
        self.write.close().await;
    }
}

/// Limit a database file, and its WAL companions, to the account that owns it. The database
/// holds password hashes, API tokens and TLS private keys, so no other local account should be
/// able to read it or a copy of it. Best effort: a failure is logged and startup continues.
pub fn restrict_to_owner(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for suffix in ["", "-wal", "-shm"] {
            let mut file = path.as_os_str().to_owned();
            file.push(suffix);
            match std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(%error, file = %file.to_string_lossy(), "could not restrict database permissions");
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn base_options(path: &Utf8Path, opts: &DbOptions) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .pragma("temp_store", "MEMORY")
        .pragma("cache_size", format!("-{}", opts.cache_kib))
}

async fn has_pending_migrations(pool: &SqlitePool) -> Result<bool> {
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    let applied: Vec<i64> = if table_exists {
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1")
            .fetch_all(pool)
            .await?
    } else {
        Vec::new()
    };
    Ok(MIGRATOR.iter().any(|m| !applied.contains(&m.version)))
}

async fn backup_before_migrate(pool: &SqlitePool, db_path: &Utf8Path) -> Result<Utf8PathBuf> {
    let dir = db_path.parent().unwrap_or(Utf8Path::new(".")).join("backups");
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(format!("pre-migrate-{}.db", mbl_core::time::now_ms()));
    sqlx::query("VACUUM INTO ?").bind(target.as_str()).execute(pool).await?;
    restrict_to_owner(target.as_std_path());
    tracing::info!(backup = %target, "backed up database before migrating");
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_scales_with_memory() {
        let gib = 1024 * 1024 * 1024;
        assert_eq!(DbOptions::for_memory(Some(2 * gib), 4).cache_kib, 8 * 1024);
        assert_eq!(DbOptions::for_memory(Some(8 * gib), 4).cache_kib, 32 * 1024);
        assert_eq!(DbOptions::for_memory(Some(64 * gib), 4).cache_kib, 64 * 1024);
        assert_eq!(DbOptions::for_memory(None, 1).read_connections, 2);
    }
}
