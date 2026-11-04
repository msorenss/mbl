//! Song lyrics (M11.1 "synced lyrics"): `.lrc` sidecars and embedded lyric tags.

use std::path::{Path, PathBuf};

use mbl_core::{Actor, Error, ItemId, MediaKind, Result};
use mbl_db::Db;
use mbl_db::repo::items::{self, ItemFilter};
use mbl_db::repo::media;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const MAX_SIDECAR_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LyricsSource {
    /// `.lrc` file next to the audio file.
    Sidecar,
    /// Lyrics tag embedded in the audio file (read during the scan).
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    /// Start time in milliseconds; absent for unsynced lyrics.
    pub start_ms: Option<u64>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResponse {
    pub item_id: ItemId,
    /// True when every line has a `startMs` (LRC timestamps were present).
    pub synced: bool,
    pub source: LyricsSource,
    pub lines: Vec<LyricLine>,
}

/// Lyrics for a track the caller may access; a sidecar `.lrc` wins over embedded tags.
pub(crate) async fn lyrics_for_item(db: &Db, actor: &Actor, id: ItemId) -> Result<LyricsResponse> {
    let detail =
        items::detail(db, &ItemFilter::for_user(actor), id).await?.ok_or(Error::NotFound)?;
    if detail.kind != MediaKind::Track {
        return Err(Error::NotFound);
    }
    for part in media::parts_for_item(db, id).await? {
        if let Some(text) = read_sidecar(Path::new(&part.path)).await {
            let (synced, lines) = parse_lrc(&text);
            if !lines.is_empty() {
                return Ok(LyricsResponse {
                    item_id: id,
                    synced,
                    source: LyricsSource::Sidecar,
                    lines,
                });
            }
        }
    }
    let embedded = items::embedded_lyrics(db, id).await?;
    let (synced, lines) = embedded.as_deref().map(parse_lrc).unwrap_or_default();
    if lines.is_empty() {
        return Err(Error::NotFound);
    }
    Ok(LyricsResponse { item_id: id, synced, source: LyricsSource::Embedded, lines })
}

fn sidecar_candidates(audio: &Path) -> Vec<PathBuf> {
    ["lrc", "LRC"].iter().map(|ext| audio.with_extension(ext)).collect()
}

async fn read_sidecar(audio: &Path) -> Option<String> {
    for candidate in sidecar_candidates(audio) {
        let Ok(meta) = tokio::fs::metadata(&candidate).await else { continue };
        if !meta.is_file() || meta.len() > MAX_SIDECAR_BYTES {
            continue;
        }
        if let Ok(bytes) = tokio::fs::read(&candidate).await {
            return Some(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_owned());
        }
    }
    None
}

/// Parse LRC text. Returns `(synced, lines)`.
///
/// Supports multiple timestamps per line, `[offset:±ms]`, metadata tags and strips
/// enhanced word timings (`<mm:ss.xx>`). Text without any timestamps is returned as
/// unsynced lines.
pub fn parse_lrc(text: &str) -> (bool, Vec<LyricLine>) {
    let mut offset_ms: i64 = 0;
    let mut timed: Vec<(i64, String)> = Vec::new();
    let mut plain: Vec<String> = Vec::new();

    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        let mut is_tag = false;
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let inner = &stripped[..end];
            if let Some(ms) = parse_timestamp(inner) {
                stamps.push(ms);
            } else if let Some((key, value)) = inner.split_once(':') {
                if key.trim().eq_ignore_ascii_case("offset") {
                    offset_ms = value.trim().parse().unwrap_or(0);
                }
                is_tag = true;
            } else {
                break;
            }
            rest = stripped[end + 1..].trim_start();
        }
        let line = strip_word_timings(rest).trim().to_owned();
        if !stamps.is_empty() {
            for stamp in stamps {
                timed.push((stamp, line.clone()));
            }
        } else if !is_tag && !line.is_empty() {
            plain.push(line);
        }
    }

    if timed.is_empty() {
        return (false, plain.into_iter().map(|text| LyricLine { start_ms: None, text }).collect());
    }
    timed.sort_by_key(|(ms, _)| *ms);
    // LRC defines a positive offset as "lyrics appear earlier".
    let lines = timed
        .into_iter()
        .map(|(ms, text)| LyricLine {
            start_ms: Some(u64::try_from((ms - offset_ms).max(0)).unwrap_or(0)),
            text,
        })
        .collect();
    (true, lines)
}

fn parse_timestamp(inner: &str) -> Option<i64> {
    let (min, sec) = inner.trim().split_once(':')?;
    let minutes: i64 = min.parse().ok()?;
    let (whole, frac) = sec.split_once(['.', ':']).unwrap_or((sec, ""));
    if whole.len() != 2 || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let seconds: i64 = whole.parse().ok()?;
    if seconds >= 60 || !frac.bytes().all(|b| b.is_ascii_digit()) || frac.len() > 3 {
        return None;
    }
    let frac_ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 100,
        2 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse::<i64>().ok()?,
    };
    Some(minutes * 60_000 + seconds * 1000 + frac_ms)
}

fn strip_word_timings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(end) if parse_timestamp(&rest[start + 1..start + end]).is_some() => {
                rest = &rest[start + end + 1..];
            }
            _ => {
                out.push('<');
                rest = &rest[start + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_lrc_with_tags_offset_and_repeats() {
        let text = "\u{feff}[ar:Artist]\n[offset:+500]\n[00:12.50][01:02.00]Chorus\n[00:05.3]<00:05.30>First <00:06.00>line\n\n[00:20]Bridge";
        let (synced, lines) = parse_lrc(text.trim_start_matches('\u{feff}'));
        assert!(synced);
        let got: Vec<_> = lines.iter().map(|l| (l.start_ms.unwrap(), l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![(4800, "First line"), (12_000, "Chorus"), (19_500, "Bridge"), (61_500, "Chorus")]
        );
    }

    #[test]
    fn plain_text_is_unsynced() {
        let (synced, lines) = parse_lrc("Hello\n\nWorld <3\n");
        assert!(!synced);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].text, "World <3");
        assert!(lines.iter().all(|l| l.start_ms.is_none()));
    }

    #[test]
    fn invalid_timestamps_are_text_and_offset_never_goes_negative() {
        let (synced, lines) = parse_lrc("[offset:5000]\n[00:01.00]Early\n[ab:cd]x");
        assert!(synced);
        assert_eq!(lines, vec![LyricLine { start_ms: Some(0), text: "Early".into() }]);
        assert_eq!(parse_timestamp("1:75.00"), None);
        assert_eq!(parse_timestamp("03:04.123"), Some(184_123));
    }
}
