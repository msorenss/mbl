use std::sync::LazyLock;

use regex::Regex;

use crate::common::{components, re, stem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtraKind {
    Trailer,
    Featurette,
    BehindTheScenes,
    DeletedScene,
    Interview,
    Scene,
    Short,
    Other,
}

static SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)[-_ .](trailer|featurette|behindthescenes|deleted|deletedscene|interview|scene|short|other|extra)$",
    )
});
static SAMPLE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)(?:^|[ ._\-\[(])sample(?:[ ._\-\])]|$)"));

fn kind_from_word(word: &str) -> Option<ExtraKind> {
    Some(match word.to_ascii_lowercase().as_str() {
        "trailer" | "trailers" => ExtraKind::Trailer,
        "featurette" | "featurettes" => ExtraKind::Featurette,
        "behindthescenes" | "behind the scenes" => ExtraKind::BehindTheScenes,
        "deleted" | "deletedscene" | "deleted scenes" => ExtraKind::DeletedScene,
        "interview" | "interviews" => ExtraKind::Interview,
        "scene" | "scenes" => ExtraKind::Scene,
        "short" | "shorts" => ExtraKind::Short,
        "other" | "others" | "extra" | "extras" => ExtraKind::Other,
        _ => return None,
    })
}

/// Classify Plex-style extras: `Movie-trailer.mkv` or files inside `Trailers/`, `Featurettes/`, ...
pub fn classify_extra(path: &str) -> Option<ExtraKind> {
    let comps = components(path);
    let (file, dirs) = comps.split_last()?;
    let name = stem(file);
    if let Some(c) = SUFFIX.captures(name) {
        return kind_from_word(&c[1]);
    }
    if name.eq_ignore_ascii_case("trailer") {
        return Some(ExtraKind::Trailer);
    }
    dirs.last().and_then(|d| kind_from_word(d))
}

/// Release samples (`movie-sample.mkv`, `Sample/…`) that should never be imported.
pub fn is_sample(path: &str) -> bool {
    let comps = components(path);
    let Some((file, dirs)) = comps.split_last() else {
        return false;
    };
    SAMPLE.is_match(stem(file)) || dirs.last().is_some_and(|d| d.eq_ignore_ascii_case("sample"))
}
