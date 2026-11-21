//! Speech to text on the server, for talking to the chat instead of typing.
//!
//! Runs Whisper on a Hailo-10H NPU when a Whisper `.hef` is installed, so speech never leaves
//! the house. Without one the web app uses the browser's own speech recognition instead and says
//! so. Reading replies aloud happens in the browser.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use mbl_core::{Actor, Error, Result};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

/// Largest recording accepted (about a minute of compressed speech).
pub const MAX_AUDIO_BYTES: usize = 4 * 1024 * 1024;
/// Longest speech transcribed.
const MAX_SECONDS: usize = 60;
const DECODE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    /// The server can turn speech into text (Whisper on the NPU).
    pub server_recognition: bool,
    /// Which model, e.g. `Whisper-Base.hef`.
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub text: String,
    pub engine: String,
}

#[derive(Debug, Clone)]
pub struct SpeechService {
    ffmpeg: PathBuf,
}

impl Default for SpeechService {
    fn default() -> Self {
        Self::new()
    }
}

/// The installed Whisper model, if any.
fn whisper_model() -> Option<PathBuf> {
    let fp =
        mbl_hwaccel::detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
    mbl_hwaccel::hailo::discover_whisper_hef_model(Path::new("/"), fp.npu_profile)
}

/// Raw little-endian f32 samples as floats.
fn samples(bytes: &[u8]) -> Vec<f32> {
    bytes.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect()
}

impl SpeechService {
    pub fn new() -> Self {
        Self { ffmpeg: PathBuf::from("ffmpeg") }
    }

    /// Use this FFmpeg to decode recordings.
    pub fn with_ffmpeg(mut self, ffmpeg: PathBuf) -> Self {
        self.ffmpeg = ffmpeg;
        self
    }

    pub async fn status(&self) -> SpeechStatus {
        let model = tokio::task::spawn_blocking(whisper_model).await.ok().flatten();
        SpeechStatus {
            server_recognition: model.is_some(),
            engine: model.and_then(|m| m.file_name().and_then(|n| n.to_str()).map(str::to_owned)),
        }
    }

    /// Recording (WebM/Ogg/MP4/WAV from the browser) to 16 kHz mono floats.
    async fn decode(&self, audio: &[u8]) -> Result<Vec<f32>> {
        let mut child = tokio::process::Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i", "pipe:0"])
            .args([
                "-t",
                &MAX_SECONDS.to_string(),
                "-ac",
                "1",
                "-ar",
                "16000",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Unavailable(format!("FFmpeg could not start: {e}")))?;
        let mut stdin = child.stdin.take().ok_or_else(|| Error::Internal("no stdin".into()))?;
        let input = audio.to_vec();
        let writer = tokio::spawn(async move {
            let _ = stdin.write_all(&input).await;
        });
        let output = tokio::time::timeout(DECODE_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| Error::Unavailable("decoding the recording took too long".into()))?
            .map_err(Error::internal)?;
        let _ = writer.await;
        // At least a quarter of a second of audio.
        if !output.status.success() || output.stdout.len() < 16_000 {
            return Err(Error::Validation(
                "The recording could not be read or is too short".into(),
            ));
        }
        Ok(samples(&output.stdout))
    }

    /// Speech to text with Whisper on the NPU. `language` is a two-letter code or `None` to let
    /// the model decide.
    pub async fn transcribe(
        &self,
        actor: &Actor,
        audio: &[u8],
        language: Option<&str>,
    ) -> Result<Transcript> {
        if actor.is_managed {
            return Err(Error::Forbidden);
        }
        if audio.is_empty() || audio.len() > MAX_AUDIO_BYTES {
            return Err(Error::Validation("Send a recording of up to about a minute".into()));
        }
        let Some(model) = tokio::task::spawn_blocking(whisper_model).await.ok().flatten() else {
            return Err(Error::Unavailable(
                "speech recognition on the server needs a Whisper model on a Hailo-10H".into(),
            ));
        };
        let pcm = self.decode(audio).await?;
        let language = language
            .map(|l| l.trim().to_ascii_lowercase())
            .filter(|l| l.len() == 2 && l.chars().all(|c| c.is_ascii_lowercase()));
        let result = tokio::task::spawn_blocking(move || {
            mbl_hailo::HailoEngine::default().transcribe_audio(&pcm, language.as_deref(), false)
        })
        .await
        .map_err(Error::internal)?;
        let (segments, _) =
            result.ok_or_else(|| Error::Unavailable("the NPU could not transcribe this".into()))?;
        let text = segments
            .iter()
            .map(|s| s.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(Transcript {
            text,
            engine: model.file_name().and_then(|n| n.to_str()).unwrap_or("Whisper").to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_little_endian_floats() {
        let bytes: Vec<u8> = [0.5f32, -1.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        assert_eq!(samples(&bytes), vec![0.5, -1.0]);
    }
}
