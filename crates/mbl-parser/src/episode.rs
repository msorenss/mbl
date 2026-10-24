use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::common::{
    components, extract_ids, join_title, junk_start, re, stem, strip_leading_group, title_and_year,
    tokenize,
};
use crate::{EpisodeGuess, ExternalIds};

static SXXEYY: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?:^|[^a-z0-9])(?P<m>s(?P<s>\d{1,3})[ ._-]?e(?P<e>\d{1,4})(?P<t>(?:-?e\d{1,4}|-\d{1,4}\b)*))",
    )
});
static NXNN: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?:^|[^a-z0-9])(?P<m>(?P<s>\d{1,2})x(?P<e>\d{2,3})(?P<t>(?:-\d{2,3}\b|x\d{2,3})*))(?:[^a-z0-9]|$)",
    )
});
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?:^|[^0-9])(?P<m>(?P<y>(?:19|20)\d{2})[-. _](?P<mo>\d{2})[-. _](?P<d>\d{2}))(?:[^0-9]|$)")
});
static EP_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?:^|[^a-z0-9])(?P<m>(?:e|ep|episode)[ ._-]?(?P<e>\d{1,4}))(?:[^0-9]|$)")
});
static ABSOLUTE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|\s)(?P<m>-\s+(?P<e>\d{1,4})(?:v\d)?)(?:\s|\[|\(|$)"));
static SEASON_DIR: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)^(?:season|series|saison|staffel|temporada|s)[ ._-]*(\d{1,4})(?:\s*\(\d{4}\))?$")
});
static SPECIALS_DIR: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^specials?$"));
static NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"(?P<sep>-)?[ex]?(?P<n>\d+)"));

/// Max episodes a range like `E01-E40` may expand to.
const MAX_RANGE: u32 = 30;

fn season_of_dir(dir: &str) -> Option<u32> {
    if SPECIALS_DIR.is_match(dir) {
        return Some(0);
    }
    SEASON_DIR.captures(dir).and_then(|c| c[1].parse().ok())
}

/// Episode numbers from the first number plus a tail like `-E03`, `E02E03` or `-03` (ranges).
fn episode_list(first: u32, tail: &str) -> Vec<u32> {
    let mut eps = vec![first];
    for cap in NUMBER.captures_iter(&tail.to_ascii_lowercase()) {
        let Ok(n) = cap["n"].parse::<u32>() else {
            continue;
        };
        let prev = eps.last().copied().unwrap_or(n);
        if cap.name("sep").is_some() && n > prev && n - prev <= MAX_RANGE {
            eps.extend(prev + 1..=n);
        } else if !eps.contains(&n) {
            eps.push(n);
        }
    }
    eps
}

struct Marker<'a> {
    prefix: &'a str,
    suffix: &'a str,
    season: Option<u32>,
    episodes: Vec<u32>,
    absolute: Option<u32>,
    air_date: Option<time::Date>,
}

fn split<'a>(s: &'a str, caps: &Captures<'_>) -> (&'a str, &'a str) {
    caps.name("m")
        .map_or((s, ""), |m| (&s[..m.start()], &s[m.end()..]))
}

fn find_marker(s: &str, has_season_dir: bool) -> Option<Marker<'_>> {
    let num = |c: &Captures<'_>, k: &str| c.name(k).and_then(|m| m.as_str().parse::<u32>().ok());

    for pattern in [&*SXXEYY, &*NXNN] {
        if let Some(c) = pattern.captures(s) {
            let (prefix, suffix) = split(s, &c);
            let first = num(&c, "e")?;
            let tail = c.name("t").map_or("", |m| m.as_str());
            return Some(Marker {
                prefix,
                suffix,
                season: num(&c, "s"),
                episodes: episode_list(first, tail),
                absolute: None,
                air_date: None,
            });
        }
    }
    if let Some(c) = DATE.captures(s) {
        let y: i32 = c["y"].parse().ok()?;
        let month = time::Month::try_from(c["mo"].parse::<u8>().ok()?).ok()?;
        let date = time::Date::from_calendar_date(y, month, c["d"].parse().ok()?).ok()?;
        let (prefix, suffix) = split(s, &c);
        return Some(Marker {
            prefix,
            suffix,
            season: None,
            episodes: Vec::new(),
            absolute: None,
            air_date: Some(date),
        });
    }
    if let Some(c) = EP_ONLY.captures(s) {
        let (prefix, suffix) = split(s, &c);
        return Some(Marker {
            prefix,
            suffix,
            season: None,
            episodes: vec![num(&c, "e")?],
            absolute: None,
            air_date: None,
        });
    }
    if let Some(c) = ABSOLUTE.captures(s) {
        let (prefix, suffix) = split(s, &c);
        let n = num(&c, "e")?;
        // Inside a season folder " - 05" is a plain episode number, otherwise absolute numbering.
        let (episodes, absolute) = if has_season_dir {
            (vec![n], None)
        } else {
            (Vec::new(), Some(n))
        };
        return Some(Marker {
            prefix,
            suffix,
            season: None,
            episodes,
            absolute,
            air_date: None,
        });
    }
    None
}

fn episode_title(suffix: &str) -> Option<String> {
    let tokens = tokenize(suffix);
    let title = join_title(&tokens[..junk_start(&tokens, 0)]);
    let title = title.trim_start_matches(['-', ' ', '.']).trim();
    (!title.is_empty()).then(|| title.to_owned())
}

fn show_from_name(name: &str) -> (String, Option<i32>, ExternalIds) {
    let (name, ids) = extract_ids(&strip_leading_group(name));
    let (show, year) = title_and_year(&name);
    (show, year, ids)
}

fn same_title(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// Guess an episode from its path, e.g. `TV/Show (2019)/Season 01/Show - S01E02 - Title.mkv`.
/// The show name comes from the folder above a season folder when present, otherwise from the
/// text before the episode marker (or the parent folder as a last resort).
pub fn parse_episode(path: &str) -> EpisodeGuess {
    let comps = components(path);
    let Some((file, dirs)) = comps.split_last() else {
        return EpisodeGuess::default();
    };
    let (name, file_ids) = extract_ids(&strip_leading_group(stem(file)));

    let season_dir = dirs.iter().rposition(|d| season_of_dir(d).is_some());
    let folder_season = season_dir.and_then(|i| season_of_dir(dirs[i]));
    let marker = find_marker(&name, season_dir.is_some());

    let prefix_show = marker.as_ref().map(|m| show_from_name(m.prefix));
    let folder_show = match season_dir {
        Some(i) => i.checked_sub(1).map(|j| show_from_name(dirs[j])),
        None => dirs.last().map(|d| show_from_name(d)),
    };

    let (show, show_year, show_ids) = match (season_dir, &prefix_show, &folder_show) {
        (Some(_), _, Some(folder)) => folder.clone(),
        (None, Some(prefix), Some(folder)) if !prefix.0.is_empty() => {
            // Flat layout: trust the file, but borrow the folder's year/ids when it is the same show.
            if same_title(&prefix.0, &folder.0) {
                (
                    prefix.0.clone(),
                    prefix.1.or(folder.1),
                    prefix.2.clone().or(folder.2.clone()),
                )
            } else {
                prefix.clone()
            }
        }
        (None, Some(prefix), None) => prefix.clone(),
        (_, _, Some(folder)) => folder.clone(),
        (_, Some(prefix), None) => prefix.clone(),
        (_, None, None) => (String::new(), None, ExternalIds::default()),
    };

    let Some(m) = marker else {
        return EpisodeGuess {
            show,
            show_year,
            ids: file_ids.or(show_ids),
            season: folder_season,
            ..Default::default()
        };
    };
    EpisodeGuess {
        show,
        show_year,
        ids: file_ids.or(show_ids),
        season: m.season.or(folder_season),
        episodes: m.episodes,
        absolute: m.absolute,
        air_date: m.air_date,
        title: episode_title(m.suffix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_expand() {
        assert_eq!(episode_list(1, "-e03"), [1, 2, 3]);
        assert_eq!(episode_list(1, "e02e03"), [1, 2, 3]);
        assert_eq!(episode_list(5, "-06"), [5, 6]);
        assert_eq!(episode_list(5, ""), [5]);
    }
}
