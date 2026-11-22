//! Hailo-Ollama: small language models on a Hailo-10H, managed from the Hardware page.
//!
//! Hailo ships `hailo-ollama` with its GenAI packages: an Ollama-compatible server (also
//! `/v1/chat/completions`, which the assistant's "Hailo-Ollama" preset uses) for the models Hailo
//! has compiled for the board (Qwen 2/2.5/3, Llama 3.2 1B, DeepSeek R1 1.5B). MBL starts it as its
//! own process on 127.0.0.1:8000 with the shared NPU group, keeps its models next to the database,
//! downloads and removes models through its API, and stops it when the NPU switch is turned off.
//! A `hailo-ollama` started by someone else on that port is used as it is.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mbl_core::{Actor, Error, Result};
use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const PROGRAM: &str = "hailo-ollama";
const DEFAULT_ADDRESS: &str = "127.0.0.1:8000";
/// Tests point MBL at a stand-in server.
const URL_SETTING: &str = "hailo_ollama_url";
/// Started from MBL: start again after a restart.
const AUTOSTART_SETTING: &str = "hailo_ollama_autostart";
const MANIFESTS: &str = "/usr/share/hailo-ollama/models/manifests";

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoOllamaModel {
    /// For example `qwen2.5:1.5b`; what the assistant's model field takes.
    pub name: String,
    pub installed: bool,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoOllamaPull {
    pub model: String,
    /// What Hailo-Ollama says it is doing.
    pub status: String,
    pub completed: u64,
    pub total: Option<u64>,
    pub finished: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoOllamaStatus {
    /// `hailo-ollama` is installed on this computer.
    pub installed: bool,
    pub running: bool,
    /// Started by MBL (and stopped by it).
    pub managed: bool,
    pub version: Option<String>,
    /// Address for the assistant's settings, e.g. `http://127.0.0.1:8000/v1`.
    pub assistant_url: String,
    pub models: Vec<HailoOllamaModel>,
    pub pull: Option<HailoOllamaPull>,
}

#[derive(Debug, Clone)]
pub struct HailoOllamaService {
    db: Db,
    http: reqwest::Client,
    child: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    pull: Arc<Mutex<Option<HailoOllamaPull>>>,
}

fn program_path() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|dir| dir.join(PROGRAM)).find(|p| p.is_file())
    })
}

impl HailoOllamaService {
    pub fn new(db: Db) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        Self { db, http, child: Arc::default(), pull: Arc::default() }
    }

    async fn base(&self) -> String {
        SettingsRepo::new(self.db.clone())
            .get(URL_SETTING)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| format!("http://{DEFAULT_ADDRESS}"))
    }

    async fn data_dir(&self) -> Result<PathBuf> {
        let file = mbl_db::repo::maintenance::db_file(&self.db)
            .await?
            .ok_or_else(|| Error::Validation("The database is not a file".into()))?;
        Ok(Path::new(&file).parent().unwrap_or(Path::new(".")).join("hailo-ollama"))
    }

    async fn version(&self, base: &str) -> Option<String> {
        let resp = self
            .http
            .get(format!("{base}/api/version"))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .ok()?;
        let body: serde_json::Value = resp.json().await.ok()?;
        body["version"].as_str().map(str::to_owned)
    }

    /// Model names from the packaged manifests (`family/size/manifest.json`), for when the
    /// server is not running.
    fn packaged_catalog() -> Vec<String> {
        let mut out = Vec::new();
        let Ok(families) = std::fs::read_dir(MANIFESTS) else { return out };
        for family in families.flatten() {
            let Ok(sizes) = std::fs::read_dir(family.path()) else { continue };
            for size in sizes.flatten() {
                out.push(format!(
                    "{}:{}",
                    family.file_name().to_string_lossy(),
                    size.file_name().to_string_lossy()
                ));
            }
        }
        out.sort();
        out
    }

    pub async fn status(&self, actor: &Actor) -> Result<HailoOllamaStatus> {
        actor.require_admin()?;
        let pull = self.pull.lock().ok().and_then(|p| p.clone());
        let base = self.base().await;
        let version = self.version(&base).await;
        let managed =
            self.child.lock().await.as_mut().is_some_and(|c| c.try_wait().ok().flatten().is_none());
        let mut names = Vec::new();
        let mut installed = std::collections::HashMap::new();
        if version.is_some() {
            if let Ok(resp) = self.http.get(format!("{base}/hailo/v1/list")).send().await
                && let Ok(body) = resp.json::<serde_json::Value>().await
            {
                names = body["models"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default();
            }
            if let Ok(resp) = self.http.get(format!("{base}/api/tags")).send().await
                && let Ok(body) = resp.json::<serde_json::Value>().await
            {
                for model in body["models"].as_array().into_iter().flatten() {
                    if let Some(name) = model["name"].as_str().or(model["model"].as_str()) {
                        installed.insert(name.to_owned(), model["size"].as_u64());
                    }
                }
            }
        }
        if names.is_empty() {
            names = Self::packaged_catalog();
        }
        for name in installed.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let models = names
            .into_iter()
            .map(|name| HailoOllamaModel {
                installed: installed.contains_key(&name),
                size_bytes: installed.get(&name).copied().flatten(),
                name,
            })
            .collect();
        Ok(HailoOllamaStatus {
            installed: program_path().is_some() || base != format!("http://{DEFAULT_ADDRESS}"),
            running: version.is_some(),
            managed,
            version,
            assistant_url: format!("{base}/v1"),
            models,
            pull,
        })
    }

    /// Start `hailo-ollama` (unless something already answers on its port).
    pub async fn start(&self, actor: &Actor) -> Result<HailoOllamaStatus> {
        actor.require_admin()?;
        if !mbl_hailo::npu_enabled() {
            return Err(Error::Conflict("MBL may not use the NPU; turn it on first".into()));
        }
        self.launch().await?;
        SettingsRepo::new(self.db.clone()).set(AUTOSTART_SETTING, "true").await?;
        self.status(actor).await
    }

    async fn launch(&self) -> Result<()> {
        let base = self.base().await;
        if self.version(&base).await.is_some() {
            return Ok(());
        }
        let program = program_path().ok_or_else(|| {
            Error::Unavailable(
                "hailo-ollama is not installed (it comes with hailo-gen-ai-model-zoo)".into(),
            )
        })?;
        let home = self.data_dir().await?;
        tokio::fs::create_dir_all(&home).await.map_err(Error::internal)?;
        let child = tokio::process::Command::new(program)
            .env("HOME", &home)
            .env("OLLAMA_HOST", DEFAULT_ADDRESS)
            // Share the board with Frigate and MBL's own models.
            .env("HAILO_OLLAMA_VDEVICE_GROUP_ID", "SHARED")
            .current_dir(&home)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Unavailable(format!("hailo-ollama could not start: {e}")))?;
        *self.child.lock().await = Some(child);
        for _ in 0..40 {
            if self.version(&base).await.is_some() {
                tracing::info!(target: "mbl::npu", "Started Hailo-Ollama");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        self.kill().await;
        Err(Error::Unavailable("hailo-ollama did not answer after starting".into()))
    }

    async fn kill(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
            tracing::info!(target: "mbl::npu", "Stopped Hailo-Ollama");
        }
    }

    /// Stop the `hailo-ollama` MBL started (one started elsewhere is left alone).
    pub async fn stop(&self, actor: &Actor) -> Result<HailoOllamaStatus> {
        actor.require_admin()?;
        self.kill().await;
        SettingsRepo::new(self.db.clone()).set(AUTOSTART_SETTING, "false").await?;
        self.status(actor).await
    }

    /// The NPU switch was turned off: let go of the board.
    pub async fn npu_turned_off(&self) {
        self.kill().await;
    }

    /// At startup: start again if MBL had it running.
    pub async fn resume(&self) {
        let wanted = SettingsRepo::new(self.db.clone())
            .get(AUTOSTART_SETTING)
            .await
            .ok()
            .flatten()
            .is_some_and(|v| v == "true");
        if wanted
            && mbl_hailo::npu_enabled()
            && let Err(error) = self.launch().await
        {
            tracing::warn!(%error, "Hailo-Ollama did not start");
        }
    }

    /// Start downloading `model` (one at a time). Progress is in `status`.
    pub async fn pull(&self, actor: &Actor, model: &str) -> Result<()> {
        actor.require_admin()?;
        let model = model.trim().to_owned();
        if model.is_empty()
            || model.len() > 64
            || !model.chars().all(|c| c.is_ascii_alphanumeric() || "._:-".contains(c))
        {
            return Err(Error::Validation("invalid model name".into()));
        }
        let base = self.base().await;
        if self.version(&base).await.is_none() {
            return Err(Error::Conflict("Hailo-Ollama is not running; start it first".into()));
        }
        {
            let mut lock = self.pull.lock().map_err(|_| Error::Internal("lock".into()))?;
            if lock.as_ref().is_some_and(|p| !p.finished) {
                return Err(Error::Conflict("a model is already downloading".into()));
            }
            *lock = Some(HailoOllamaPull { model: model.clone(), ..Default::default() });
        }
        let service = self.clone();
        tokio::spawn(async move {
            let outcome = service.run_pull(&base, &model).await;
            if let Ok(mut lock) = service.pull.lock()
                && let Some(p) = lock.as_mut()
            {
                p.finished = true;
                p.error = outcome.err().map(|e| {
                    let text = e.to_string();
                    text.split_once(": ").map(|(_, r)| r.to_owned()).unwrap_or(text)
                });
            }
        });
        Ok(())
    }

    async fn run_pull(&self, base: &str, model: &str) -> Result<()> {
        let mut resp = self
            .http
            .post(format!("{base}/api/pull"))
            .json(&serde_json::json!({ "model": model, "stream": true }))
            .send()
            .await
            .map_err(|e| Error::External(format!("Hailo-Ollama did not answer: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::External(format!("Hailo-Ollama said {}", resp.status())));
        }
        // Newline-separated JSON progress, as Ollama sends it.
        let mut pending = String::new();
        loop {
            let chunk = tokio::time::timeout(Duration::from_secs(120), resp.chunk())
                .await
                .map_err(|_| Error::External("The download stalled".into()))?
                .map_err(|e| Error::External(format!("The download broke off: {e}")))?;
            let Some(chunk) = chunk else { break };
            pending.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(end) = pending.find('\n') {
                let line: String = pending.drain(..=end).collect();
                self.progress(&line)?;
            }
        }
        if !pending.trim().is_empty() {
            self.progress(&pending)?;
        }
        tracing::info!(target: "mbl::npu", npu_model = %model, "Hailo-Ollama model installed");
        Ok(())
    }

    fn progress(&self, line: &str) -> Result<()> {
        let Ok(update) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            return Ok(());
        };
        if let Some(error) = update["error"].as_str() {
            return Err(Error::External(error.to_owned()));
        }
        if let Ok(mut lock) = self.pull.lock()
            && let Some(p) = lock.as_mut()
        {
            if let Some(status) = update["status"].as_str() {
                p.status = status.chars().take(80).collect();
            }
            if let Some(total) = update["total"].as_u64() {
                p.total = Some(total);
            }
            if let Some(done) = update["completed"].as_u64() {
                p.completed = done;
            }
        }
        Ok(())
    }

    pub async fn delete(&self, actor: &Actor, model: &str) -> Result<()> {
        actor.require_admin()?;
        let base = self.base().await;
        let resp = self
            .http
            .delete(format!("{base}/api/delete"))
            .json(&serde_json::json!({ "model": model }))
            .send()
            .await
            .map_err(|e| Error::External(format!("Hailo-Ollama did not answer: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::NotFound);
        }
        if !resp.status().is_success() {
            return Err(Error::External(format!("Hailo-Ollama said {}", resp.status())));
        }
        Ok(())
    }
}
