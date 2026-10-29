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
    pub dvr: DvrConfig,
    pub log: LogConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DvrConfig {
    pub min_free_mb: u64,
    pub max_recording_mb: Option<u64>,
}
impl Default for DvrConfig {
    fn default() -> Self {
        Self { min_free_mb: 512, max_recording_mb: None }
    }
}
impl DvrConfig {
    pub fn policy(&self) -> anyhow::Result<mbl_core::livetv::DvrStoragePolicy> {
        let min_free_bytes = self
            .min_free_mb
            .checked_mul(1024 * 1024)
            .ok_or_else(|| anyhow::anyhow!("DVR min_free_mb is too large"))?;
        let max_recording_bytes = self
            .max_recording_mb
            .map(|n| {
                n.checked_mul(1024 * 1024).filter(|n| *n > 0).ok_or_else(|| {
                    anyhow::anyhow!("DVR max_recording_mb must be positive and fit in bytes")
                })
            })
            .transpose()?;
        Ok(mbl_core::livetv::DvrStoragePolicy { min_free_bytes, max_recording_bytes })
    }
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

    /// Resolve effective limits for the given [`mbl_platform::PlatformProfile`] (M5.2),
    /// using profile defaults unless explicitly overridden in config.
    pub fn effective_for_profile(
        &self,
        profile: mbl_platform::PlatformProfile,
    ) -> mbl_platform::PlatformDefaults {
        let mut out = profile.defaults();
        if self.scanner.probe_concurrency > 0 {
            out.probe_concurrency = self.scanner.probe_concurrency;
        }
        if self.transcode.max_sessions > 0 {
            out.max_transcode_sessions = self.transcode.max_sessions;
        }
        if self.transcode.segment_secs > 0
            && self.transcode.segment_secs != TranscodeConfig::default().segment_secs
        {
            out.segment_secs = self.transcode.segment_secs;
        }
        if self.transcode.ahead_segments > 0
            && self.transcode.ahead_segments != TranscodeConfig::default().ahead_segments
        {
            out.ahead_segments = self.transcode.ahead_segments;
        }
        out
    }

    /// Resolve transcode directory and quota using M5.3 tmpfs rules (`/run/mbl/transcode` capped at 25% RAM when Linux RAM >= 4 GiB).
    pub fn transcode_selection(
        &self,
        is_linux: bool,
        total_ram_bytes: Option<u64>,
    ) -> mbl_platform::TranscodeDirSelection {
        let data =
            non_empty(&self.paths.data_dir).unwrap_or_else(mbl_platform::dirs::default_data_dir);
        mbl_platform::dirs::select_transcode_dir(
            &self.paths.transcode_dir,
            &data,
            is_linux,
            total_ram_bytes,
            self.transcode.temp_quota_mb,
        )
    }
}

fn non_empty(s: &str) -> Option<Utf8PathBuf> {
    (!s.trim().is_empty()).then(|| Utf8Path::new(s.trim()).to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn dvr_policy_defaults_and_invalid_limits() {
        let defaults = super::DvrConfig::default().policy().expect("defaults");
        assert_eq!(defaults.min_free_bytes, 512 * 1024 * 1024);
        assert!(defaults.max_recording_bytes.is_none());
        assert!(
            super::DvrConfig { min_free_mb: u64::MAX, max_recording_mb: None }.policy().is_err()
        );
        assert!(super::DvrConfig { min_free_mb: 0, max_recording_mb: Some(0) }.policy().is_err());
    }
    use super::*;
    use mbl_platform::PlatformProfile;

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

    #[test]
    fn profile_defaults_and_config_overrides_m5_2() {
        let cfg = Config::default();
        let pi5 = cfg.effective_for_profile(PlatformProfile::Pi5);
        assert_eq!(pi5.probe_concurrency, 2);
        assert_eq!(pi5.segment_secs, 3);
        assert_eq!(pi5.ahead_segments, 6);
        assert_eq!(pi5.max_transcode_sessions, 1);

        let intel = cfg.effective_for_profile(PlatformProfile::X86Intel);
        assert_eq!(intel.probe_concurrency, 8);
        assert_eq!(intel.segment_secs, 6);
        assert_eq!(intel.ahead_segments, 10);
        assert_eq!(intel.max_transcode_sessions, 4);

        let mut custom = Config::default();
        custom.scanner.probe_concurrency = 5;
        custom.transcode.max_sessions = 3;
        custom.transcode.segment_secs = 4;
        custom.transcode.ahead_segments = 12;
        let overridden = custom.effective_for_profile(PlatformProfile::Pi5);
        assert_eq!(overridden.probe_concurrency, 5);
        assert_eq!(overridden.max_transcode_sessions, 3);
        assert_eq!(overridden.segment_secs, 4);
        assert_eq!(overridden.ahead_segments, 12);
    }

    #[test]
    fn tmpfs_transcode_selection_m5_3() {
        let cfg = Config::default();
        let sel = cfg.transcode_selection(true, Some(8 * 1024 * 1024 * 1024));
        assert_eq!(sel.path, "/run/mbl/transcode");
        assert!(sel.is_tmpfs);
        assert_eq!(sel.quota_bytes, 2 * 1024 * 1024 * 1024);
    }
}
