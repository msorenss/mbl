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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_absolute() {
        assert!(default_config_file().is_absolute());
        assert!(default_data_dir().is_absolute());
        assert_eq!(default_config_file().file_name(), Some("mbl.toml"));
    }
}
