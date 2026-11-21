//! AI models for a Hailo NPU, installed from the Hardware page.
//!
//! MBL knows which Hailo-10H models it uses (a vision-language model for scene descriptions and
//! Whisper for speech), fetches the build that matches the installed HailoRT from Hailo's
//! download server into `<data dir>/models` (where the NPU code looks), checks it and shows what
//! is installed and in use. Installing the driver and HailoRT itself is left to the system.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mbl_core::{Actor, Error, Result};
use mbl_db::Db;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

/// Where Hailo publishes compiled GenAI models, per HailoRT version.
const DOWNLOAD_BASE: &str = "https://dev-public.hailo.ai";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HailoModelRole {
    /// Describes what is on screen.
    Vision,
    /// Turns speech into text.
    Speech,
}

struct CatalogEntry {
    file: &'static str,
    role: HailoModelRole,
    name: &'static str,
    description: &'static str,
    recommended: bool,
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        file: "Qwen2-VL-2B-Instruct.hef",
        role: HailoModelRole::Vision,
        name: "Qwen2-VL 2B",
        description: "Describes the picture in a sentence for the AI companion and the assistant.",
        recommended: true,
    },
    CatalogEntry {
        file: "Qwen3-VL-2B-Instruct.hef",
        role: HailoModelRole::Vision,
        name: "Qwen3-VL 2B",
        description: "Newer scene description model; use it instead of Qwen2-VL, not both.",
        recommended: false,
    },
    CatalogEntry {
        file: "Whisper-Base.hef",
        role: HailoModelRole::Speech,
        name: "Whisper Base",
        description: "Speech to text for Talk in the chats, on the server.",
        recommended: true,
    },
    CatalogEntry {
        file: "Whisper-Small.hef",
        role: HailoModelRole::Speech,
        name: "Whisper Small",
        description: "More accurate speech to text, slower; use it instead of Whisper Base.",
        recommended: false,
    },
];

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoModel {
    pub file: String,
    pub role: HailoModelRole,
    pub name: String,
    pub description: String,
    pub recommended: bool,
    /// Download size, when Hailo's server answered.
    pub size_bytes: Option<u64>,
    pub installed: bool,
    /// The NPU code would use this file for its role.
    pub active: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoModelDownload {
    pub file: String,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub finished: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoModelsStatus {
    /// `hailo10H`, `hailo8`, … or none.
    pub npu: Option<String>,
    /// Models can be installed here (a Hailo-10H with a known HailoRT version).
    pub supported: bool,
    /// Why not, when not.
    pub reason: Option<String>,
    pub hailort_version: Option<String>,
    pub directory: String,
    pub free_bytes: Option<u64>,
    pub models: Vec<HailoModel>,
    pub download: Option<HailoModelDownload>,
}

/// Download sizes per file for one HailoRT version.
type KnownSizes = (String, Vec<(String, u64)>);

#[derive(Clone)]
pub struct HailoModelService {
    db: Db,
    http: reqwest::Client,
    download: Arc<Mutex<Option<HailoModelDownload>>>,
    sizes: Arc<Mutex<Option<KnownSizes>>>,
}

impl std::fmt::Debug for HailoModelService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HailoModelService").finish_non_exhaustive()
    }
}

/// `HailoRT-CLI version 5.4.0` → `5.4.0`; otherwise the version in `libhailort.so.X.Y.Z`.
fn hailort_version() -> Option<String> {
    let from_cli = std::process::Command::new("hailortcli")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|text| parse_version(&text));
    from_cli.or_else(|| {
        ["/usr/lib", "/usr/lib/aarch64-linux-gnu", "/usr/lib/x86_64-linux-gnu", "/usr/local/lib"]
            .iter()
            .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter_map(|n| n.strip_prefix("libhailort.so.").and_then(parse_version))
            .max()
    })
}

/// The last `X.Y.Z` in a text.
fn parse_version(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace())
        .rev()
        .map(|w| w.trim_start_matches('v'))
        .find(|w| {
            let parts: Vec<&str> = w.split('.').collect();
            parts.len() == 3
                && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .map(str::to_owned)
}

fn url(version: &str, file: &str) -> String {
    format!("{DOWNLOAD_BASE}/v{version}/blob/{file}")
}

/// `<data dir>/models`, next to the database (the NPU code looks in `/var/lib/mbl/models`).
async fn models_dir(db: &Db) -> Result<PathBuf> {
    let file = mbl_db::repo::maintenance::db_file(db)
        .await?
        .ok_or_else(|| Error::Validation("The database is not a file".into()))?;
    Ok(Path::new(&file).parent().unwrap_or(Path::new(".")).join("models"))
}

impl HailoModelService {
    pub fn new(db: Db) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .user_agent(concat!("MBL/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self { db, http, download: Arc::default(), sizes: Arc::default() }
    }

    fn entry(file: &str) -> Result<&'static CatalogEntry> {
        CATALOG.iter().find(|e| e.file == file).ok_or(Error::NotFound)
    }

    /// Download sizes for this HailoRT version (asked once, then remembered).
    async fn sizes(&self, version: &str) -> Vec<(String, u64)> {
        if let Ok(lock) = self.sizes.lock()
            && let Some((v, sizes)) = lock.as_ref()
            && v == version
        {
            return sizes.clone();
        }
        let mut found = Vec::new();
        for entry in CATALOG {
            let head = self
                .http
                .head(url(version, entry.file))
                .timeout(Duration::from_secs(8))
                .send()
                .await;
            if let Ok(resp) = head
                && resp.status().is_success()
                && let Some(len) = resp.content_length()
            {
                found.push((entry.file.to_owned(), len));
            }
        }
        if !found.is_empty()
            && let Ok(mut lock) = self.sizes.lock()
        {
            *lock = Some((version.to_owned(), found.clone()));
        }
        found
    }

    pub async fn status(&self, actor: &Actor) -> Result<HailoModelsStatus> {
        actor.require_admin()?;
        let (profile, version) = tokio::task::spawn_blocking(|| {
            (mbl_hwaccel::detect_fingerprint().npu_profile, hailort_version())
        })
        .await
        .map_err(Error::internal)?;
        let npu = serde_json::to_value(profile).ok().and_then(|v| v.as_str().map(str::to_owned));
        let ten = profile == mbl_hwaccel::NpuProfile::Hailo10H;
        let reason = if !ten {
            Some(match profile {
                mbl_hwaccel::NpuProfile::None => "No Hailo NPU was found.".to_owned(),
                _ => "These models need a Hailo-10H; a Hailo-8 runs only detection models such as YOLO.".to_owned(),
            })
        } else if version.is_none() {
            Some("HailoRT is not installed (hailortcli was not found).".to_owned())
        } else {
            None
        };
        let dir = models_dir(&self.db).await?;
        let sizes = match (&version, ten) {
            (Some(v), true) => self.sizes(v).await,
            _ => Vec::new(),
        };
        let root = Path::new("/");
        let active = [
            mbl_hwaccel::hailo::discover_vlm_hef_model(root, profile),
            mbl_hwaccel::hailo::discover_whisper_hef_model(root, profile),
        ];
        let models = CATALOG
            .iter()
            .map(|e| {
                let path = dir.join(e.file);
                HailoModel {
                    file: e.file.to_owned(),
                    role: e.role,
                    name: e.name.to_owned(),
                    description: e.description.to_owned(),
                    recommended: e.recommended,
                    size_bytes: sizes.iter().find(|(f, _)| f == e.file).map(|(_, s)| *s),
                    installed: path.is_file(),
                    active: active.iter().flatten().any(|a| a.file_name() == path.file_name()),
                }
            })
            .collect();
        let free_bytes = std::fs::create_dir_all(&dir)
            .ok()
            .and_then(|()| mbl_platform::disk::available_bytes(&dir).ok());
        Ok(HailoModelsStatus {
            npu,
            supported: reason.is_none(),
            reason,
            hailort_version: version,
            directory: dir.display().to_string(),
            free_bytes,
            models,
            download: self.download.lock().ok().and_then(|d| d.clone()),
        })
    }

    /// Start downloading a model (one at a time). Progress is in `status`.
    pub async fn download(&self, actor: &Actor, file: &str) -> Result<()> {
        let status = self.status(actor).await?;
        let entry = Self::entry(file)?;
        if !status.supported {
            return Err(Error::Validation(status.reason.unwrap_or_default()));
        }
        let version = status.hailort_version.unwrap_or_default();
        let dir = PathBuf::from(&status.directory);
        let size = status.models.iter().find(|m| m.file == file).and_then(|m| m.size_bytes);
        if let (Some(size), Some(free)) = (size, status.free_bytes)
            && free < size + 1024 * 1024 * 1024
        {
            return Err(Error::Validation(format!(
                "Not enough free space in {} (needs {} MB plus 1 GB to spare)",
                dir.display(),
                size / (1024 * 1024)
            )));
        }
        {
            let mut lock = self.download.lock().map_err(|_| Error::Internal("lock".into()))?;
            if lock.as_ref().is_some_and(|d| !d.finished) {
                return Err(Error::Conflict("a model is already downloading".into()));
            }
            *lock = Some(HailoModelDownload {
                file: entry.file.to_owned(),
                total_bytes: size,
                ..Default::default()
            });
        }
        let service = self.clone();
        let file = entry.file.to_owned();
        tokio::spawn(async move {
            let outcome = service.fetch(&version, &dir, &file).await;
            if let Ok(mut lock) = service.download.lock()
                && let Some(d) = lock.as_mut()
            {
                d.finished = true;
                d.error = outcome
                    .err()
                    .map(|e| e.to_string().trim_start_matches("validation: ").to_owned());
            }
        });
        Ok(())
    }

    async fn fetch(&self, version: &str, dir: &Path, file: &str) -> Result<()> {
        tokio::fs::create_dir_all(dir).await.map_err(Error::internal)?;
        let target = dir.join(file);
        let part = dir.join(format!("{file}.part"));
        let mut resp = self.http.get(url(version, file)).send().await.map_err(|e| {
            Error::External(format!("Could not reach Hailo's download server: {}", e.without_url()))
        })?;
        if !resp.status().is_success() {
            return Err(Error::External(format!(
                "Hailo's server has no {file} for HailoRT {version} ({})",
                resp.status()
            )));
        }
        let total = resp.content_length();
        let mut out = tokio::fs::File::create(&part).await.map_err(Error::internal)?;
        let mut received: u64 = 0;
        loop {
            let chunk = tokio::time::timeout(Duration::from_secs(60), resp.chunk())
                .await
                .map_err(|_| Error::External("The download stalled".into()))?
                .map_err(|e| {
                    Error::External(format!("The download broke off: {}", e.without_url()))
                })?;
            let Some(chunk) = chunk else { break };
            out.write_all(&chunk).await.map_err(Error::internal)?;
            received += chunk.len() as u64;
            if let Ok(mut lock) = self.download.lock()
                && let Some(d) = lock.as_mut()
            {
                d.received_bytes = received;
                d.total_bytes = total;
            }
        }
        out.flush().await.map_err(Error::internal)?;
        drop(out);
        if total.is_some_and(|t| t != received) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::External("The download is incomplete".into()));
        }
        // HailoRT must be able to read it, or it is no use.
        let check = tokio::process::Command::new("hailortcli")
            .arg("parse-hef")
            .arg(&part)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .status();
        if let Ok(Ok(status)) = tokio::time::timeout(Duration::from_secs(120), check).await
            && !status.success()
        {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::External("HailoRT could not read the downloaded model".into()));
        }
        tokio::fs::rename(&part, &target).await.map_err(Error::internal)?;
        tracing::info!(file, bytes = received, "Hailo model installed");
        Ok(())
    }

    /// Remove an installed model.
    pub async fn remove(&self, actor: &Actor, file: &str) -> Result<()> {
        actor.require_admin()?;
        let entry = Self::entry(file)?;
        if self
            .download
            .lock()
            .ok()
            .and_then(|d| d.clone())
            .is_some_and(|d| d.file == file && !d.finished)
        {
            return Err(Error::Conflict("this model is downloading".into()));
        }
        let path = models_dir(&self.db).await?.join(entry.file);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::NotFound),
            Err(e) => Err(Error::internal(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_addresses() {
        assert_eq!(parse_version("HailoRT-CLI version 5.4.0\n").as_deref(), Some("5.4.0"));
        assert_eq!(parse_version("5.5.0").as_deref(), Some("5.5.0"));
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(
            url("5.4.0", "Whisper-Base.hef"),
            "https://dev-public.hailo.ai/v5.4.0/blob/Whisper-Base.hef"
        );
        assert!(HailoModelService::entry("../etc/passwd").is_err());
    }
}
