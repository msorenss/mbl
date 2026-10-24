use std::sync::LazyLock;

use regex::Regex;

use crate::common::{
    components, extract_edition, extract_ids, re, stem, strip_leading_group, title_and_year,
};
use crate::{ExternalIds, MovieGuess};

static PART: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)[\s._-]+(?:cd|disc|disk|dvd|part|pt)[\s._-]*(\d{1,2})$"));

struct NameGuess {
    title: String,
    year: Option<i32>,
    ids: ExternalIds,
    edition: Option<String>,
    part: Option<u32>,
}

fn parse_name(name: &str) -> NameGuess {
    let name = strip_leading_group(name);
    let (name, ids) = extract_ids(&name);
    let (name, edition) = extract_edition(&name);
    let trimmed = name.trim_end();
    let (name, part) = match PART.captures(trimmed) {
        Some(cap) => {
            let start = cap.get(0).map_or(trimmed.len(), |m| m.start());
            (trimmed[..start].to_owned(), cap[1].parse().ok())
        }
        None => (trimmed.to_owned(), None),
    };
    let (title, year) = title_and_year(&name);
    NameGuess { title, year, ids, edition, part }
}

/// Guess a movie from its path, e.g. `Movies/Heat (1995) {tmdb-949}/Heat (1995).mkv`.
/// The file name wins when it has a year; otherwise the parent folder is used.
pub fn parse_movie(path: &str) -> MovieGuess {
    let comps = components(path);
    let Some(file) = comps.last() else {
        return MovieGuess::default();
    };
    let file = parse_name(stem(file));
    let folder = comps.len().checked_sub(2).map(|i| parse_name(comps[i]));

    let (title, year) = match &folder {
        Some(f) if file.year.is_none() && f.year.is_some() && !f.title.is_empty() => {
            (f.title.clone(), f.year)
        }
        Some(f) if file.title.is_empty() => (f.title.clone(), f.year),
        _ => (file.title.clone(), file.year),
    };
    let (folder_ids, folder_edition) =
        folder.map_or((ExternalIds::default(), None), |f| (f.ids, f.edition));

    MovieGuess {
        title,
        year,
        ids: file.ids.or(folder_ids),
        edition: file.edition.or(folder_edition),
        part: file.part,
    }
}
