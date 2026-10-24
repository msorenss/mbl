use std::sync::LazyLock;

use regex::Regex;

use crate::MusicGuess;
use crate::common::{as_year, components, re, stem};

static DISC_DIR: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)^(?:disc|disk|cd)[ ._-]*(\d{1,2})(?:\s*[-:].*)?$"));
static DISC_TRACK: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?P<d>\d{1,2})[-.](?P<t>\d{2,3})(?:[ ._-]+(?P<rest>.+))?$"));
static TRACK: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?P<t>\d{1,3})(?:\s*[-.]\s*|\s+|_)(?P<rest>.+)$"));
static ALBUM_YEAR_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?P<a>.+?)\s*[\(\[](?P<y>\d{4})[\)\]]\s*$"));
static ALBUM_YEAR_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^(?:[\(\[](?P<by>\d{4})[\)\]]\s*(?:-\s*)?|(?P<y>\d{4})\s*-\s*)(?P<a>.+)$")
});

fn album_and_year(dir: &str) -> (String, Option<i32>) {
    if let Some(c) = ALBUM_YEAR_SUFFIX.captures(dir)
        && let Some(y) = as_year(&c["y"])
    {
        return (c["a"].trim().to_owned(), Some(y));
    }
    if let Some(c) = ALBUM_YEAR_PREFIX.captures(dir) {
        let year = c.name("by").or_else(|| c.name("y")).and_then(|m| as_year(m.as_str()));
        if let Some(y) = year {
            return (c["a"].trim().to_owned(), Some(y));
        }
    }
    (dir.trim().to_owned(), None)
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// Guess artist/album/disc/track/title from `Artist/Album (Year)/[Disc N/]NN - Title.ext`.
/// Embedded tags are authoritative; this is the fallback when tags are missing.
pub fn parse_music(path: &str) -> MusicGuess {
    let comps = components(path);
    let Some((file, dirs)) = comps.split_last() else {
        return MusicGuess::default();
    };
    let mut dirs = dirs.to_vec();
    let mut disc =
        dirs.last().and_then(|d| DISC_DIR.captures(d)).and_then(|c| c[1].parse::<u32>().ok());
    if disc.is_some() {
        dirs.pop();
    }
    let (album, album_year) = match dirs.last() {
        Some(d) => {
            let (a, y) = album_and_year(d);
            (non_empty(&a), y)
        }
        None => (None, None),
    };
    let artist = dirs.len().checked_sub(2).and_then(|i| non_empty(dirs[i]));

    let name = stem(file).trim();
    let (track, rest) = if let Some(c) = DISC_TRACK.captures(name) {
        disc = disc.or(c["d"].parse().ok());
        (c["t"].parse().ok(), c.name("rest").map_or("", |m| m.as_str()).to_owned())
    } else if let Some(c) = TRACK.captures(name) {
        (c["t"].parse().ok(), c["rest"].to_owned())
    } else {
        (None, name.to_owned())
    };

    // "Artist - Title" in the file name: drop the artist part when it matches the folder.
    let title = match (rest.split_once(" - "), &artist) {
        (Some((a, t)), Some(artist)) if a.trim().eq_ignore_ascii_case(artist) => non_empty(t),
        _ => non_empty(&rest),
    };

    MusicGuess { artist, album, album_year, disc, track, title }
}
