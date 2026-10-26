//! Default per-OS locations (IMPLEMENTATION.md §1.3).

use camino::Utf8PathBuf;

/// Default config file path.
pub fn default_config_file() -> Utf8PathBuf {
    if cfg!(windows) {
        program_data().join("mbl.toml")
    } else {
        Utf8PathBuf::from("/etc/mbl/mbl.toml")
    }
}

/// Default data directory (database, logs, backups, caches).
pub fn default_data_dir() -> Utf8PathBuf {
    if cfg!(windows) { program_data().join("data") } else { Utf8PathBuf::from("/var/lib/mbl") }
}

fn program_data() -> Utf8PathBuf {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_owned());
    Utf8PathBuf::from(base).join("MBL")
}

/// Minimum host RAM (4 GiB) required to place the transcode directory on `/run/mbl/transcode` tmpfs on Linux (M5.3).
pub const TMPFS_MIN_RAM_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Default Linux tmpfs transcode path when RAM >= 4 GiB (M5.3).
pub const LINUX_TMPFS_TRANSCODE_DIR: &str = "/run/mbl/transcode";

/// Result of resolving the transcode directory and its disk/tmpfs quota (M5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscodeDirSelection {
    pub path: Utf8PathBuf,
    pub is_tmpfs: bool,
    /// Size cap in bytes (25% of RAM when using automatic tmpfs, otherwise `configured_quota_mb * 1 MiB`).
    pub quota_bytes: u64,
}

/// Select the transcode scratch directory and size cap (M5.3).
///
/// When `configured_dir` is empty on Linux and `total_ram_bytes >= 4 GiB`, selects `/run/mbl/transcode`
/// with a quota capped at 25% of total RAM.
pub fn select_transcode_dir(
    configured_dir: &str,
    data_dir: &camino::Utf8Path,
    is_linux: bool,
    total_ram_bytes: Option<u64>,
    configured_quota_mb: u64,
) -> TranscodeDirSelection {
    let fallback_quota = configured_quota_mb.saturating_mul(1024 * 1024);
    let trimmed = configured_dir.trim();
    if !trimmed.is_empty() {
        return TranscodeDirSelection {
            path: Utf8PathBuf::from(trimmed),
            is_tmpfs: false,
            quota_bytes: fallback_quota,
        };
    }

    if is_linux
        && let Some(ram) = total_ram_bytes
        && ram >= TMPFS_MIN_RAM_BYTES
    {
        return TranscodeDirSelection {
            path: Utf8PathBuf::from(LINUX_TMPFS_TRANSCODE_DIR),
            is_tmpfs: true,
            quota_bytes: ram / 4,
        };
    }

    TranscodeDirSelection {
        path: data_dir.join("transcode"),
        is_tmpfs: false,
        quota_bytes: fallback_quota,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;

    #[test]
    fn defaults_are_absolute() {
        assert!(default_config_file().is_absolute());
        assert!(default_data_dir().is_absolute());
        assert_eq!(default_config_file().file_name(), Some("mbl.toml"));
    }

    #[test]
    fn linux_with_4gb_or_more_uses_tmpfs_capped_at_25_percent_ram() {
        let ram_8gb = 8 * 1024 * 1024 * 1024;
        let sel =
            select_transcode_dir("", Utf8Path::new("/var/lib/mbl"), true, Some(ram_8gb), 4096);
        assert_eq!(sel.path, "/run/mbl/transcode");
        assert!(sel.is_tmpfs);
        assert_eq!(sel.quota_bytes, 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn linux_under_4gb_or_non_linux_uses_data_dir_transcode() {
        let ram_2gb = 2 * 1024 * 1024 * 1024;
        let sel =
            select_transcode_dir("", Utf8Path::new("/var/lib/mbl"), true, Some(ram_2gb), 4096);
        assert_eq!(sel.path, "/var/lib/mbl/transcode");
        assert!(!sel.is_tmpfs);
        assert_eq!(sel.quota_bytes, 4096 * 1024 * 1024);

        let win_sel = select_transcode_dir(
            "",
            Utf8Path::new("C:/ProgramData/MBL/data"),
            false,
            Some(16 * 1024 * 1024 * 1024),
            4096,
        );
        assert_eq!(win_sel.path, "C:/ProgramData/MBL/data/transcode");
        assert!(!win_sel.is_tmpfs);
    }

    #[test]
    fn explicit_config_transcode_dir_overrides_tmpfs() {
        let sel = select_transcode_dir(
            "/mnt/nvme/transcode",
            Utf8Path::new("/var/lib/mbl"),
            true,
            Some(8 * 1024 * 1024 * 1024),
            8192,
        );
        assert_eq!(sel.path, "/mnt/nvme/transcode");
        assert!(!sel.is_tmpfs);
        assert_eq!(sel.quota_bytes, 8192 * 1024 * 1024);
    }
}
