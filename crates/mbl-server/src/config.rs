//! Layered configuration: defaults → TOML file → `MBL_` env vars → CLI flags.

use camino::{Utf8Path, Utf8PathBuf};
use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub paths: PathsConfig,
    pub scanner: ScannerConfig,
    pub metadata: MetadataConfig,
    pub transcode: TranscodeConfig,
    pub log: LogConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    pub allow_wan: bool,
    pub allowed_origins: Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self { enabled: true, allow_wan: false, allowed_origins: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
    pub port: u16,
    /// Optional externally reachable URL, used for links and casting.
    pub public_url: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { bind: "0.0.0.0".into(), port: 8420, public_url: String::new() }
    }
}

/// Empty strings mean "use the platform default".
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PathsConfig {
    pub data_dir: String,
    pub cache_dir: String,
    pub transcode_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ScannerConfig {
    /// 0 = automatic, based on the platform profile.
    pub probe_concurrency: u32,
    /// Seconds between rescans of roots on network filesystems.
    pub poll_interval_secs: u64,
    /// Watch local library roots for changes.
    pub watch: bool,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self { probe_concurrency: 0, poll_interval_secs: 900, watch: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MetadataConfig {
    pub language: String,
    pub tmdb_api_key: String,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self { language: "en-US".into(), tmdb_api_key: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TranscodeConfig {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    /// 0 = automatic, based on measured capacity.
    pub max_sessions: u32,
    pub segment_secs: u32,
    pub ahead_segments: u32,
    pub temp_quota_mb: u64,
}

impl Default for TranscodeConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
            max_sessions: 0,
            segment_secs: 6,
            ahead_segments: 10,
            temp_quota_mb: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LogConfig {
    /// `tracing` EnvFilter directive, e.g. `info` or `info,mbl_db=debug`.
    pub level: String,
    pub json: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self { level: "info".into(), json: false }
    }
}

/// Values given on the command line; they override every other source.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub config_file: Option<Utf8PathBuf>,
    pub data_dir: Option<Utf8PathBuf>,
    pub port: Option<u16>,
}

/// Fully resolved filesystem locations.
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    pub data: Utf8PathBuf,
    pub cache: Utf8PathBuf,
    pub transcode: Utf8PathBuf,
    pub logs: Utf8PathBuf,
    pub db_file: Utf8PathBuf,
}

impl Config {
    pub fn load(cli: &CliOverrides) -> Result<Self, Box<figment::Error>> {
        Self::load_with_env(cli, Env::prefixed("MBL_").split("__"))
    }

    fn load_with_env(cli: &CliOverrides, env: Env) -> Result<Self, Box<figment::Error>> {
        let file = cli.config_file.clone().unwrap_or_else(mbl_platform::dirs::default_config_file);
        let mut figment = Figment::from(Serialized::defaults(Config::default()));
        // A missing config file is fine: defaults + env + CLI are enough to run.
        if file.exists() {
            figment = figment.merge(Toml::file_exact(file.as_std_path()));
        }
        figment = figment.merge(env);
        if let Some(port) = cli.port {
            figment = figment.merge(("server.port", port));
        }
        if let Some(dir) = &cli.data_dir {
            figment = figment.merge(("paths.data_dir", dir.as_str()));
        }
        figment.extract().map_err(Box::new)
    }

    pub fn paths(&self) -> Paths {
        let data =
            non_empty(&self.paths.data_dir).unwrap_or_else(mbl_platform::dirs::default_data_dir);
        let cache = non_empty(&self.paths.cache_dir).unwrap_or_else(|| data.join("cache"));
        let transcode =
            non_empty(&self.paths.transcode_dir).unwrap_or_else(|| data.join("transcode"));
        Paths { logs: data.join("logs"), db_file: data.join("mbl.db"), data, cache, transcode }
    }
}

fn non_empty(s: &str) -> Option<Utf8PathBuf> {
    (!s.trim().is_empty()).then(|| Utf8Path::new(s.trim()).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env() -> Env {
        Env::prefixed("MBL_TEST_UNUSED_PREFIX_")
    }

    fn write_toml(dir: &tempfile::TempDir, body: &str) -> Utf8PathBuf {
        let path = Utf8PathBuf::from_path_buf(dir.path().join("mbl.toml")).unwrap();
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn defaults_when_file_missing() {
        let cli = CliOverrides {
            config_file: Some("/nonexistent/mbl.toml".into()),
            ..Default::default()
        };
        let cfg = Config::load_with_env(&cli, no_env()).unwrap();
        assert_eq!(cfg, Config::default());
        assert_eq!(cfg.server.port, 8420);
    }

    #[test]
    fn file_then_cli_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_toml(&dir, "[server]\nport = 9000\n[log]\nlevel = \"debug\"\n");
        let mut cli = CliOverrides { config_file: Some(file), ..Default::default() };

        let cfg = Config::load_with_env(&cli, no_env()).unwrap();
        assert_eq!(cfg.server.port, 9000);
        assert_eq!(cfg.log.level, "debug");

        cli.port = Some(9100);
        cli.data_dir = Some("/tmp/mbl-data".into());
        let cfg = Config::load_with_env(&cli, no_env()).unwrap();
        assert_eq!(cfg.server.port, 9100);
        assert_eq!(cfg.paths.data_dir, "/tmp/mbl-data");
    }

    #[test]
    #[allow(clippy::result_large_err)] // figment::Jail's closure signature is fixed
    fn env_overrides_file() {
        figment::Jail::expect_with(|jail| {
            jail.create_file("mbl.toml", "[server]\nport = 9000\n")?;
            jail.set_env("MBL_SERVER__PORT", "9200");
            jail.set_env("MBL_METADATA__TMDB_API_KEY", "abc");
            let cli = CliOverrides { config_file: Some("mbl.toml".into()), ..Default::default() };
            let cfg = Config::load(&cli).map_err(|e| *e)?;
            assert_eq!(cfg.server.port, 9200);
            assert_eq!(cfg.metadata.tmdb_api_key, "abc");
            Ok(())
        });
    }

    #[test]
    fn invalid_value_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_toml(&dir, "[server]\nport = \"not a number\"\n");
        let cli = CliOverrides { config_file: Some(file), ..Default::default() };
        assert!(Config::load_with_env(&cli, no_env()).is_err());
    }

    #[test]
    fn paths_resolve_relative_to_data_dir() {
        let mut cfg = Config::default();
        cfg.paths.data_dir = "/srv/mbl".into();
        let p = cfg.paths();
        assert_eq!(p.db_file, "/srv/mbl/mbl.db");
        assert_eq!(p.cache, "/srv/mbl/cache");
        assert_eq!(p.transcode, "/srv/mbl/transcode");
        assert_eq!(p.logs, "/srv/mbl/logs");

        cfg.paths.cache_dir = "/fast/cache".into();
        assert_eq!(cfg.paths().cache, "/fast/cache");
    }
}
