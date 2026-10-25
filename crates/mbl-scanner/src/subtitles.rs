//! External sidecar subtitle discovery (`Movie.en.srt`, `Movie.en.forced.srt`, `Movie.eng.sdh.srt`).

use std::path::Path;

use mbl_core::StreamKind;
use mbl_probe::StreamInfo;

/// Recognized external subtitle file extensions and their normalized codec names.
pub const SUBTITLE_EXTS: &[(&str, &str)] =
    &[("srt", "srt"), ("ass", "ass"), ("ssa", "ass"), ("vtt", "vtt"), ("sub", "subrip")];

/// Parsed metadata from a sidecar subtitle filename suffix (e.g. `.en.forced.srt` or `.eng.sdh.srt`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SidecarMeta {
    pub codec: String,
    pub language: Option<String>,
    pub forced: bool,
    pub default: bool,
    pub sdh: bool,
}

/// Discover external sidecar subtitle files sitting alongside `media_path` and build
/// [`StreamInfo`] descriptors numbered starting from `next_stream_index`.
pub fn discover_sidecars(media_path: &Path, next_stream_index: u32) -> Vec<StreamInfo> {
    let Some(parent) = media_path.parent() else {
        return Vec::new();
    };
    let Some(media_stem) = media_path.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };

    let mut matches: Vec<(String, SidecarMeta)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|ft| ft.is_file()) {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(path_str) = path.to_str() else {
            continue;
        };
        if let Some(meta) = parse_sidecar_filename(media_stem, file_name) {
            matches.push((path_str.to_owned(), meta));
        }
    }

    matches.sort_by(|a, b| a.0.cmp(&b.0));

    matches
        .into_iter()
        .enumerate()
        .map(|(offset, (path_str, meta))| StreamInfo {
            index: next_stream_index.saturating_add(u32::try_from(offset).unwrap_or(0)),
            kind: StreamKind::Subtitle,
            codec: meta.codec,
            profile: None,
            level: None,
            language: meta.language,
            title: meta.sdh.then(|| "SDH".to_owned()),
            default: meta.default,
            forced: meta.forced,
            external_path: Some(path_str),
            width: None,
            height: None,
            bit_depth: None,
            frame_rate: None,
            interlaced: false,
            hdr: None,
            channels: None,
            channel_layout: None,
            sample_rate: None,
        })
        .collect()
}

/// Parse `candidate_filename` against `media_stem`.
///
/// Examples for `media_stem = "Movie"`:
/// - `Movie.en.srt` -> `language: Some("en"), forced: false, sdh: false, codec: "srt"`
/// - `Movie.en.forced.srt` -> `language: Some("en"), forced: true, sdh: false, codec: "srt"`
/// - `Movie.eng.sdh.srt` -> `language: Some("eng"), forced: false, sdh: true, codec: "srt"`
pub fn parse_sidecar_filename(media_stem: &str, candidate_filename: &str) -> Option<SidecarMeta> {
    let (cand_stem, ext) = candidate_filename.rsplit_once('.')?;
    let codec = SUBTITLE_EXTS
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(ext))
        .map(|(_, c)| (*c).to_owned())?;

    if cand_stem.len() < media_stem.len() {
        return None;
    }
    let (prefix, rest) = cand_stem.split_at(media_stem.len());
    if !prefix.eq_ignore_ascii_case(media_stem) {
        return None;
    }

    if rest.is_empty() {
        return Some(SidecarMeta { codec, ..Default::default() });
    }
    let suffix = rest.strip_prefix('.')?;

    let mut meta = SidecarMeta { codec, ..Default::default() };
    for raw_token in suffix.split('.').filter(|t| !t.is_empty()) {
        let lower = raw_token.to_ascii_lowercase();
        match lower.as_str() {
            "forced" | "force" => meta.forced = true,
            "default" => meta.default = true,
            "sdh" | "cc" | "hi" => meta.sdh = true,
            _ if is_language_token(&lower) && meta.language.is_none() => {
                meta.language = Some(lower);
            }
            _ => {}
        }
    }

    Some(meta)
}

fn is_language_token(tok: &str) -> bool {
    if (2..=3).contains(&tok.len()) && tok.chars().all(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    if let Some((lang, region)) = tok.split_once('-') {
        return (2..=3).contains(&lang.len())
            && lang.chars().all(|c| c.is_ascii_alphabetic())
            && (2..=4).contains(&region.len())
            && region.chars().all(|c| c.is_ascii_alphanumeric());
    }
    false
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_sidecar_filenames() {
        let en = parse_sidecar_filename("Movie", "Movie.en.srt").unwrap();
        assert_eq!(
            en,
            SidecarMeta {
                codec: "srt".into(),
                language: Some("en".into()),
                forced: false,
                default: false,
                sdh: false,
            }
        );

        let forced = parse_sidecar_filename("Movie", "Movie.en.forced.srt").unwrap();
        assert_eq!(
            forced,
            SidecarMeta {
                codec: "srt".into(),
                language: Some("en".into()),
                forced: true,
                default: false,
                sdh: false,
            }
        );

        let sdh = parse_sidecar_filename("Movie", "Movie.eng.sdh.srt").unwrap();
        assert_eq!(
            sdh,
            SidecarMeta {
                codec: "srt".into(),
                language: Some("eng".into()),
                forced: false,
                default: false,
                sdh: true,
            }
        );

        let ass = parse_sidecar_filename("Show - S01E01", "Show - S01E01.jpn.default.ass").unwrap();
        assert_eq!(
            ass,
            SidecarMeta {
                codec: "ass".into(),
                language: Some("jpn".into()),
                forced: false,
                default: true,
                sdh: false,
            }
        );

        assert!(parse_sidecar_filename("Movie", "OtherMovie.en.srt").is_none());
        assert!(parse_sidecar_filename("Movie", "Movie.mkv").is_none());
    }

    #[test]
    fn discovers_sidecars_in_directory() {
        let dir = tempfile::tempdir().unwrap();
        let movie = dir.path().join("Heat (1995).mkv");
        std::fs::write(&movie, b"video").unwrap();
        std::fs::write(
            dir.path().join("Heat (1995).en.srt"),
            b"1\n00:00:01,000 --> 00:00:02,000\nHi\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Heat (1995).en.forced.srt"),
            b"1\n00:00:01,000 --> 00:00:02,000\nForced\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Heat (1995).eng.sdh.srt"),
            b"1\n00:00:01,000 --> 00:00:02,000\n[doorbell]\n",
        )
        .unwrap();

        let streams = discover_sidecars(&movie, 2);
        assert_eq!(streams.len(), 3);

        assert_eq!(streams[0].index, 2);
        assert_eq!(streams[0].language.as_deref(), Some("en"));
        assert!(streams[0].forced);
        assert!(
            streams[0]
                .external_path
                .as_ref()
                .unwrap()
                .as_str()
                .ends_with("Heat (1995).en.forced.srt")
        );

        assert_eq!(streams[1].index, 3);
        assert_eq!(streams[1].language.as_deref(), Some("en"));
        assert!(!streams[1].forced);
        assert!(
            streams[1].external_path.as_ref().unwrap().as_str().ends_with("Heat (1995).en.srt")
        );

        assert_eq!(streams[2].index, 4);
        assert_eq!(streams[2].language.as_deref(), Some("eng"));
        assert_eq!(streams[2].title.as_deref(), Some("SDH"));
        assert!(
            streams[2]
                .external_path
                .as_ref()
                .unwrap()
                .as_str()
                .ends_with("Heat (1995).eng.sdh.srt")
        );
    }
}
