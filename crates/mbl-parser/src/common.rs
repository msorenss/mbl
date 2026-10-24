//! Shared helpers: path splitting, ID/edition extraction, tokenizing and junk detection.

use std::sync::LazyLock;

use regex::Regex;

use crate::ExternalIds;

#[allow(clippy::expect_used)] // patterns are static literals covered by tests
pub(crate) fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid static regex")
}

/// Path components, split on both separator styles.
pub(crate) fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
        .collect()
}

/// File name without its extension (only short alphanumeric extensions are stripped).
pub(crate) fn stem(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((s, ext))
            if !s.is_empty()
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            s
        }
        _ => file,
    }
}

static IDS: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)[\[{]\s*(tmdb|tmdbid|imdb|imdbid|tvdb|tvdbid)\s*[-=:]\s*([a-z0-9]+)\s*[\]}]")
});

/// Remove `{tmdb-123}` / `[imdbid-tt123]` style tags and return them.
pub(crate) fn extract_ids(s: &str) -> (String, ExternalIds) {
    let mut ids = ExternalIds::default();
    for cap in IDS.captures_iter(s) {
        let value = &cap[2];
        match cap[1].to_ascii_lowercase().trim_end_matches("id") {
            "tmdb" => ids.tmdb = ids.tmdb.or(value.parse().ok()),
            "tvdb" => ids.tvdb = ids.tvdb.or(value.parse().ok()),
            _ => {
                if ids.imdb.is_none() && value.to_ascii_lowercase().starts_with("tt") {
                    ids.imdb = Some(value.to_ascii_lowercase());
                }
            }
        }
    }
    (IDS.replace_all(s, " ").into_owned(), ids)
}

static EDITION: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)[\[{]edition-([^\]}]+)[\]}]"));

pub(crate) fn extract_edition(s: &str) -> (String, Option<String>) {
    let edition = EDITION
        .captures(s)
        .map(|c| c[1].trim().to_owned())
        .filter(|e| !e.is_empty());
    (EDITION.replace_all(s, " ").into_owned(), edition)
}

static LEADING_GROUP: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*\[[^\]]*\]\s*"));

/// Remove a leading `[ReleaseGroup]` tag (common in anime releases).
pub(crate) fn strip_leading_group(s: &str) -> String {
    LEADING_GROUP.replace(s, "").into_owned()
}

/// Split a name into tokens. Names containing spaces keep dots inside words ("Mr. Robot");
/// scene-style names without spaces treat `.` and `_` as separators.
pub(crate) fn tokenize(s: &str) -> Vec<String> {
    let dots_are_separators = !s.contains(' ');
    s.split(|c: char| {
        c.is_whitespace()
            || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '_')
            || (c == '.' && dots_are_separators)
    })
    .map(|t| t.trim_matches(','))
    .filter(|t| !t.is_empty())
    .map(str::to_owned)
    .collect()
}

const JUNK: &[&str] = &[
    "4k", "uhd", "hd", "fhd", "sd", "bd", "bluray", "blu-ray", "bdrip", "brrip", "bdremux",
    "remux", "webrip", "web-dl", "webdl", "web", "hdtv", "pdtv", "dvdrip", "dvdscr", "dvd",
    "hdrip", "x264", "x265", "h264", "h265", "h.264", "h.265", "hevc", "avc", "av1", "xvid",
    "divx", "aac", "aac2.0", "ac3", "eac3", "dts", "dts-hd", "dd5.1", "ddp5.1", "atmos", "truehd",
    "flac", "10bit", "8bit", "hdr", "hdr10", "hdr10+", "dv", "dovi", "sdr", "proper", "repack",
    "internal", "limited", "multi", "subbed", "dubbed", "nf", "amzn", "dsnp", "hmax", "atvp",
    "imax", "complete",
];

static RESOLUTION: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^\d{3,4}[pi]$"));

pub(crate) fn is_junk(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    let t = t.trim_matches('-');
    JUNK.contains(&t) || RESOLUTION.is_match(t)
}

/// Index of the first junk token at or after `from`, or `tokens.len()`.
pub(crate) fn junk_start(tokens: &[String], from: usize) -> usize {
    tokens
        .iter()
        .skip(from)
        .position(|t| is_junk(t))
        .map_or(tokens.len(), |i| i + from)
}

pub(crate) fn as_year(token: &str) -> Option<i32> {
    (token.len() == 4)
        .then(|| token.parse::<i32>().ok())
        .flatten()
        .filter(|y| (1900..=2099).contains(y))
}

/// Join tokens into a display title, dropping dangling separators.
pub(crate) fn join_title(tokens: &[String]) -> String {
    let words: Vec<&str> = tokens
        .iter()
        .map(String::as_str)
        .filter(|t| !t.chars().all(|c| c == '-'))
        .collect();
    words
        .join(" ")
        .trim()
        .trim_end_matches([' ', '-', ','])
        .trim()
        .to_owned()
}

/// Title + year from a name: the year is the last plausible year token before any junk that is
/// not the first token (so "1917 (2019)" and "2001 A Space Odyssey (1968)" work).
pub(crate) fn title_and_year(name: &str) -> (String, Option<i32>) {
    let tokens = tokenize(name);
    // The first token is always part of the title ("HDR Test", "UHD Test").
    let junk = junk_start(&tokens, 1);
    let year_idx = (1..junk).rev().find(|&i| as_year(&tokens[i]).is_some());
    match year_idx {
        Some(i) => (join_title(&tokens[..i]), as_year(&tokens[i])),
        None => (join_title(&tokens[..junk]), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_editions() {
        let (rest, ids) = extract_ids("Heat (1995) {tmdb-949} [imdbid-tt0113277]");
        assert_eq!(ids.tmdb, Some(949));
        assert_eq!(ids.imdb.as_deref(), Some("tt0113277"));
        assert!(!rest.contains("tmdb"));
        let (_, ed) = extract_edition("Blade Runner (1982) {edition-Final Cut}");
        assert_eq!(ed.as_deref(), Some("Final Cut"));
    }

    #[test]
    fn tokenizer_modes() {
        assert_eq!(tokenize("Mr. Robot (2015)"), ["Mr.", "Robot", "2015"]);
        assert_eq!(
            tokenize("The.Matrix.1999.1080p"),
            ["The", "Matrix", "1999", "1080p"]
        );
        assert_eq!(stem("a.b.mkv"), "a.b");
        assert_eq!(stem("noext"), "noext");
    }
}
