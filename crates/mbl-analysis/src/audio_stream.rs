//! Progressive audio transcoding endpoint support (M7.2).
//! Transcodes on-the-fly to Opus (Ogg/WebM), AAC (ADTS/MP4), or MP3 via chunked HTTP streaming.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Opus,
    Aac,
    Mp3,
}

impl AudioCodec {
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Opus => "audio/ogg",
            Self::Aac => "audio/aac",
            Self::Mp3 => "audio/mpeg",
        }
    }

    pub fn format_name(&self) -> &'static str {
        match self {
            Self::Opus => "ogg",
            Self::Aac => "adts",
            Self::Mp3 => "mp3",
        }
    }

    pub fn encoder_name(&self) -> &'static str {
        match self {
            Self::Opus => "libopus",
            Self::Aac => "aac",
            Self::Mp3 => "libmp3lame",
        }
    }
}

/// Arguments to launch FFmpeg for progressive audio streaming to stdout (`pipe:1`).
pub fn build_audio_transcode_args(
    input_path: &Path,
    codec: AudioCodec,
    bitrate_kbps: u32,
    seek_secs: Option<f64>,
) -> Vec<String> {
    let mut args = vec!["-nostdin".into(), "-hide_banner".into(), "-v".into(), "error".into()];

    if let Some(seek) = seek_secs.filter(|&s| s > 0.0) {
        args.push("-ss".into());
        args.push(format!("{:.3}", seek));
    }

    args.extend([
        "-i".into(),
        input_path.to_string_lossy().into_owned(),
        "-vn".into(),
        "-c:a".into(),
        codec.encoder_name().into(),
        "-b:a".into(),
        format!("{}k", bitrate_kbps),
    ]);

    if codec == AudioCodec::Opus {
        args.extend(["-vbr".into(), "on".into()]);
    }

    args.extend(["-f".into(), codec.format_name().into(), "pipe:1".into()]);

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_transcode_args() {
        let args = build_audio_transcode_args(
            Path::new("/media/song.flac"),
            AudioCodec::Opus,
            160,
            Some(12.5),
        );
        assert!(args.contains(&"-ss".to_string()));
        assert!(args.contains(&"12.500".to_string()));
        assert!(args.contains(&"libopus".to_string()));
        assert!(args.contains(&"160k".to_string()));
        assert!(args.contains(&"ogg".to_string()));
    }
}
