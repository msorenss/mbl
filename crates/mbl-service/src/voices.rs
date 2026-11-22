//! Reading aloud on the server with Piper.
//!
//! Many browsers have no voices of their own (Chromium on Linux, including on a Raspberry Pi,
//! has none without a speech engine), so replies stayed silent there. Piper is a small neural
//! text-to-speech program that runs fast on a Pi's processor. An administrator installs the
//! program and a voice per language from the Hardware page; MBL keeps them next to the database
//! (`piper/` and `voices/`) and turns text into a WAV file for the browser to play.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mbl_core::{Actor, Error, Result};
use mbl_db::Db;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use utoipa::ToSchema;

const PROGRAM_RELEASE: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2";
const VOICE_BASE: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0";
/// Longest text read aloud at once.
pub const MAX_TEXT_CHARS: usize = 2_000;
const SPEAK_TIMEOUT: Duration = Duration::from_secs(60);

/// The Piper build for this computer, as published on GitHub.
fn program_archive() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "aarch64") => Some("piper_linux_aarch64.tar.gz"),
        ("linux", "x86_64") => Some("piper_linux_x86_64.tar.gz"),
        ("windows", "x86_64") => Some("piper_windows_amd64.zip"),
        _ => None,
    }
}

/// One voice per language (two-letter code as MBL uses it), picked from Piper's list:
/// medium quality, one speaker.
const VOICES: &[(&str, &str, &str)] = &[
    ("sv", "Lisa (svenska)", "sv/sv_SE/lisa/medium/sv_SE-lisa-medium"),
    ("en", "Amy (English, US)", "en/en_US/amy/medium/en_US-amy-medium"),
    ("no", "Talesyntese (norsk)", "no/no_NO/talesyntese/medium/no_NO-talesyntese-medium"),
    ("de", "Thorsten (Deutsch)", "de/de_DE/thorsten/medium/de_DE-thorsten-medium"),
    ("fr", "Siwis (français)", "fr/fr_FR/siwis/medium/fr_FR-siwis-medium"),
    ("es", "Davefx (español)", "es/es_ES/davefx/medium/es_ES-davefx-medium"),
    ("pt", "Cadu (português, BR)", "pt/pt_BR/cadu/medium/pt_BR-cadu-medium"),
    ("it", "Paola (italiano)", "it/it_IT/paola/medium/it_IT-paola-medium"),
    ("nl", "Pim (Nederlands)", "nl/nl_NL/pim/medium/nl_NL-pim-medium"),
    ("pl", "Darkman (polski)", "pl/pl_PL/darkman/medium/pl_PL-darkman-medium"),
    ("tr", "DFKI (Türkçe)", "tr/tr_TR/dfki/medium/tr_TR-dfki-medium"),
    ("ru", "Denis (русский)", "ru/ru_RU/denis/medium/ru_RU-denis-medium"),
    (
        "uk",
        "Ukrainian TTS (українська)",
        "uk/uk_UA/ukrainian_tts/medium/uk_UA-ukrainian_tts-medium",
    ),
    ("ar", "Kareem (العربية)", "ar/ar_JO/kareem/medium/ar_JO-kareem-medium"),
    ("hi", "Pratham (हिन्दी)", "hi/hi_IN/pratham/medium/hi_IN-pratham-medium"),
    ("zh", "Huayan (中文)", "zh/zh_CN/huayan/medium/zh_CN-huayan-medium"),
    ("vi", "Vais1000 (Tiếng Việt)", "vi/vi_VN/vais1000/medium/vi_VN-vais1000-medium"),
];

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerVoice {
    /// Two-letter language code.
    pub language: String,
    pub name: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoiceDownload {
    /// `program` or a language code.
    pub what: String,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub finished: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoicesStatus {
    /// Piper exists for this computer.
    pub supported: bool,
    /// The Piper program is installed.
    pub program_installed: bool,
    pub voices: Vec<ServerVoice>,
    pub download: Option<VoiceDownload>,
}

#[derive(Debug, Clone)]
pub struct VoiceService {
    db: Db,
    http: reqwest::Client,
    download: Arc<Mutex<Option<VoiceDownload>>>,
    /// One voice at a time: Piper uses all cores while it speaks.
    speaking: Arc<tokio::sync::Semaphore>,
    /// Tests point downloads elsewhere.
    base_override: Option<(String, String)>,
    /// Tests use a stand-in program.
    program_override: Option<PathBuf>,
}

impl VoiceService {
    pub fn new(db: Db) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .user_agent(concat!("MBL/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            db,
            http,
            download: Arc::default(),
            speaking: Arc::new(tokio::sync::Semaphore::new(1)),
            base_override: None,
            program_override: None,
        }
    }

    /// For tests: download from `program_base`/`voice_base` and speak with `program`.
    pub fn with_test_sources(
        mut self,
        program_base: String,
        voice_base: String,
        program: Option<PathBuf>,
    ) -> Self {
        self.base_override = Some((program_base, voice_base));
        self.program_override = program;
        self
    }

    async fn base_dir(&self) -> Result<PathBuf> {
        let file = mbl_db::repo::maintenance::db_file(&self.db)
            .await?
            .ok_or_else(|| Error::Validation("The database is not a file".into()))?;
        Ok(Path::new(&file).parent().unwrap_or(Path::new(".")).to_path_buf())
    }

    async fn program(&self) -> Result<PathBuf> {
        if let Some(program) = &self.program_override {
            return Ok(program.clone());
        }
        let exe = if cfg!(windows) { "piper.exe" } else { "piper" };
        Ok(self.base_dir().await?.join("piper").join("piper").join(exe))
    }

    async fn voice_file(&self, language: &str) -> Result<Option<PathBuf>> {
        let Some((_, _, path)) = VOICES.iter().find(|(l, _, _)| *l == language) else {
            return Ok(None);
        };
        let name = path.rsplit('/').next().unwrap_or(path);
        Ok(Some(self.base_dir().await?.join("voices").join(format!("{name}.onnx"))))
    }

    /// Languages the server can read aloud right now.
    pub async fn languages(&self) -> Vec<String> {
        if !self.program().await.is_ok_and(|p| p.is_file()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (language, _, _) in VOICES {
            if let Ok(Some(file)) = self.voice_file(language).await
                && file.is_file()
            {
                out.push((*language).to_owned());
            }
        }
        out
    }

    pub async fn status(&self, actor: &Actor) -> Result<VoicesStatus> {
        actor.require_admin()?;
        // Read the download first: a download that finishes while the files are checked must
        // not be reported as done with its file still missing.
        let download = self.download.lock().ok().and_then(|d| d.clone());
        let program_installed = self.program().await.is_ok_and(|p| p.is_file());
        let mut voices = Vec::new();
        for (language, name, _) in VOICES {
            let installed = self.voice_file(language).await?.is_some_and(|f| f.is_file());
            voices.push(ServerVoice {
                language: (*language).to_owned(),
                name: (*name).to_owned(),
                installed,
            });
        }
        Ok(VoicesStatus {
            supported: program_archive().is_some() || self.program_override.is_some(),
            program_installed,
            voices,
            download,
        })
    }

    fn begin(&self, what: &str) -> Result<()> {
        let mut lock = self.download.lock().map_err(|_| Error::Internal("lock".into()))?;
        if lock.as_ref().is_some_and(|d| !d.finished) {
            return Err(Error::Conflict("something is already downloading".into()));
        }
        *lock = Some(VoiceDownload { what: what.to_owned(), ..Default::default() });
        Ok(())
    }

    fn end(&self, outcome: Result<()>) {
        if let Ok(mut lock) = self.download.lock()
            && let Some(d) = lock.as_mut()
        {
            d.finished = true;
            d.error = outcome.err().map(|e| {
                let text = e.to_string();
                text.split_once(": ").map(|(_, rest)| rest.to_owned()).unwrap_or(text)
            });
        }
    }

    /// Start installing the Piper program (about 25 MB). Progress is in `status`.
    pub async fn install_program(&self, actor: &Actor) -> Result<()> {
        actor.require_admin()?;
        let archive = program_archive()
            .ok_or_else(|| Error::Validation("Piper is not available for this computer".into()))?;
        let base = self.base_dir().await?;
        let url = format!(
            "{}/{archive}",
            self.base_override.as_ref().map_or(PROGRAM_RELEASE, |(p, _)| p.as_str())
        );
        self.begin("program")?;
        let service = self.clone();
        tokio::spawn(async move {
            let outcome = service.fetch_program(&url, &base, archive).await;
            service.end(outcome);
        });
        Ok(())
    }

    async fn fetch_program(&self, url: &str, base: &Path, archive: &str) -> Result<()> {
        let target = base.join("piper");
        let file = base.join(archive);
        self.fetch(url, &file).await?;
        // The archive holds a `piper/` folder with the program, its libraries and espeak data.
        let _ = tokio::fs::remove_dir_all(&target).await;
        tokio::fs::create_dir_all(&target).await.map_err(Error::internal)?;
        let status = tokio::process::Command::new("tar")
            .arg(if archive.ends_with(".zip") { "-xf" } else { "-xzf" })
            .arg(&file)
            .arg("-C")
            .arg(&target)
            .kill_on_drop(true)
            .status()
            .await
            .map_err(|e| Error::Unavailable(format!("could not unpack Piper: {e}")))?;
        let _ = tokio::fs::remove_file(&file).await;
        if !status.success() {
            return Err(Error::External("The Piper download could not be unpacked".into()));
        }
        if !self.program().await?.is_file() {
            return Err(Error::External("The Piper download has no program in it".into()));
        }
        tracing::info!("Piper installed for reading aloud");
        Ok(())
    }

    /// Start installing the voice for `language` (about 60 MB). Progress is in `status`.
    pub async fn install_voice(&self, actor: &Actor, language: &str) -> Result<()> {
        actor.require_admin()?;
        let (_, _, path) = VOICES
            .iter()
            .find(|(l, _, _)| *l == language)
            .ok_or_else(|| Error::Validation(format!("no voice for `{language}`")))?;
        let model = self.voice_file(language).await?.ok_or(Error::NotFound)?;
        let base = self.base_override.as_ref().map_or(VOICE_BASE, |(_, v)| v.as_str()).to_owned();
        let path = (*path).to_owned();
        self.begin(language)?;
        let service = self.clone();
        tokio::spawn(async move {
            let outcome = async {
                let mut config = model.clone().into_os_string();
                config.push(".json");
                service.fetch(&format!("{base}/{path}.onnx.json"), Path::new(&config)).await?;
                service.fetch(&format!("{base}/{path}.onnx"), &model).await
            }
            .await;
            if outcome.is_ok() {
                tracing::info!(language = %path, "Piper voice installed");
            }
            service.end(outcome);
        });
        Ok(())
    }

    pub async fn remove_voice(&self, actor: &Actor, language: &str) -> Result<()> {
        actor.require_admin()?;
        let model = self.voice_file(language).await?.ok_or(Error::NotFound)?;
        let mut config = model.clone().into_os_string();
        config.push(".json");
        let _ = tokio::fs::remove_file(&config).await;
        match tokio::fs::remove_file(&model).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::NotFound),
            Err(e) => Err(Error::internal(e)),
        }
    }

    /// Download `url` to `target` through a `.part` file, recording progress.
    async fn fetch(&self, url: &str, target: &Path) -> Result<()> {
        if let Some(dir) = target.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(Error::internal)?;
        }
        let mut part = target.as_os_str().to_owned();
        part.push(".part");
        let part = PathBuf::from(part);
        let mut resp = self.http.get(url).send().await.map_err(|e| {
            Error::External(format!("Could not reach the download server: {}", e.without_url()))
        })?;
        if !resp.status().is_success() {
            return Err(Error::External(format!("The download server said {}", resp.status())));
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
        if total.is_some_and(|t| t != received) || received == 0 {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::External("The download is incomplete".into()));
        }
        tokio::fs::rename(&part, target).await.map_err(Error::internal)?;
        Ok(())
    }

    /// `text` read aloud in `language`, as a WAV file. `Unavailable` when the server has no
    /// voice for it.
    pub async fn speak(&self, actor: &Actor, text: &str, language: &str) -> Result<Vec<u8>> {
        if actor.is_managed {
            return Err(Error::Forbidden);
        }
        let text: String = text.chars().take(MAX_TEXT_CHARS).collect();
        if text.trim().is_empty() {
            return Err(Error::Validation("nothing to read aloud".into()));
        }
        let language = language.trim().to_ascii_lowercase();
        let program = self.program().await?;
        let model = self.voice_file(&language).await?.filter(|f| f.is_file());
        let (Some(model), true) = (model, program.is_file()) else {
            return Err(Error::Unavailable(format!(
                "the server has no voice for `{language}`; install one on the Hardware page"
            )));
        };
        let _turn = self.speaking.acquire().await.map_err(Error::internal)?;
        let out = tempfile::Builder::new()
            .prefix("mbl-voice-")
            .suffix(".wav")
            .tempfile()
            .map_err(Error::internal)?
            .into_temp_path();
        let mut command = tokio::process::Command::new(&program);
        command
            .arg("--model")
            .arg(&model)
            .arg("--output_file")
            .arg(&*out)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = program.parent() {
            // The libraries and espeak data ship next to the program.
            command.env("LD_LIBRARY_PATH", dir).current_dir(dir);
        }
        let mut child = command
            .spawn()
            .map_err(|e| Error::Unavailable(format!("Piper could not start: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).await.map_err(Error::internal)?;
        }
        let output = tokio::time::timeout(SPEAK_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| Error::Unavailable("reading aloud took too long".into()))?
            .map_err(Error::internal)?;
        if !output.status.success() {
            let reason = String::from_utf8_lossy(&output.stderr);
            let reason = reason.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            tracing::warn!(%reason, "Piper failed");
            return Err(Error::Unavailable("Piper could not read this aloud".into()));
        }
        let mut wav = Vec::new();
        tokio::fs::File::open(&*out)
            .await
            .map_err(Error::internal)?
            .read_to_end(&mut wav)
            .await
            .map_err(Error::internal)?;
        if wav.len() < 44 || &wav[..4] != b"RIFF" {
            return Err(Error::Unavailable("Piper gave no sound".into()));
        }
        Ok(wav)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_voice_is_one_of_mbls_languages_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (language, _, path) in VOICES {
            assert_eq!(language.len(), 2, "{language}");
            assert!(seen.insert(*language), "two voices for {language}");
            assert!(path.ends_with("-medium"), "{path}");
        }
    }
}
