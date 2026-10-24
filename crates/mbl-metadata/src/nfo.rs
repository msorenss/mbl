//! Kodi-style `.nfo` files and local artwork next to media files.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Credit, MetadataBundle};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Nfo {
    title: Option<String>,
    originaltitle: Option<String>,
    plot: Option<String>,
    outline: Option<String>,
    tagline: Option<String>,
    year: Option<String>,
    premiered: Option<String>,
    aired: Option<String>,
    rating: Option<String>,
    studio: Vec<String>,
    genre: Vec<String>,
    tmdbid: Option<String>,
    imdbid: Option<String>,
    imdb_id: Option<String>,
    tvdbid: Option<String>,
    id: Option<String>,
    uniqueid: Vec<UniqueId>,
    actor: Vec<Actor>,
    director: Vec<String>,
    credits: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UniqueId {
    #[serde(rename = "@type")]
    kind: Option<String>,
    #[serde(rename = "$text")]
    value: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Actor {
    name: Option<String>,
    role: Option<String>,
    thumb: Option<String>,
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// Parse a movie/tvshow/episode NFO. Returns `None` for non-XML NFOs (e.g. a bare IMDb URL).
pub fn parse_nfo(xml: &str) -> Option<MetadataBundle> {
    let nfo: Nfo = quick_xml::de::from_str(xml.trim_start_matches('\u{feff}')).ok()?;
    let mut tmdb = clean(nfo.tmdbid).and_then(|v| v.parse().ok());
    let mut tvdb = clean(nfo.tvdbid).and_then(|v| v.parse().ok());
    let mut imdb = clean(nfo.imdbid).or(clean(nfo.imdb_id));
    for u in nfo.uniqueid {
        let value = clean(u.value);
        match u.kind.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("tmdb") => tmdb = tmdb.or(value.and_then(|v| v.parse().ok())),
            Some("tvdb") => tvdb = tvdb.or(value.and_then(|v| v.parse().ok())),
            Some("imdb") => imdb = imdb.or(value),
            _ => {}
        }
    }
    let legacy_id = clean(nfo.id);
    if imdb.is_none() && legacy_id.as_deref().is_some_and(|v| v.starts_with("tt")) {
        imdb = legacy_id;
    }
    let release_date = clean(nfo.premiered).or(clean(nfo.aired));
    let year = clean(nfo.year)
        .and_then(|y| y.parse().ok())
        .or_else(|| release_date.as_deref().and_then(|d| d.get(..4)?.parse().ok()));

    let mut people: Vec<Credit> = nfo
        .actor
        .into_iter()
        .filter_map(|a| {
            Some(Credit {
                name: clean(a.name)?,
                tmdb_id: None,
                thumb_url: clean(a.thumb),
                role: "actor".into(),
                character: clean(a.role),
            })
        })
        .collect();
    for (names, role) in [(nfo.director, "director"), (nfo.credits, "writer")] {
        people.extend(names.into_iter().filter_map(|n| clean(Some(n))).map(|name| Credit {
            name,
            tmdb_id: None,
            thumb_url: None,
            role: role.into(),
            character: None,
        }));
    }

    Some(MetadataBundle {
        title: clean(nfo.title),
        original_title: clean(nfo.originaltitle),
        summary: clean(nfo.plot).or(clean(nfo.outline)),
        tagline: clean(nfo.tagline),
        year,
        release_date,
        rating: clean(nfo.rating).and_then(|r| r.parse().ok()),
        studio: nfo.studio.into_iter().find_map(|s| clean(Some(s))),
        genres: nfo.genre.into_iter().filter_map(|g| clean(Some(g))).collect(),
        people,
        posters: Vec::new(),
        backdrops: Vec::new(),
        tmdb_id: tmdb,
        tvdb_id: tvdb,
        imdb_id: imdb,
    })
}

/// First existing NFO among the candidates.
pub(crate) fn read_first_nfo(candidates: &[PathBuf]) -> Option<MetadataBundle> {
    candidates.iter().filter(|p| p.is_file()).find_map(|p| {
        let text = std::fs::read_to_string(p).ok()?;
        let parsed = parse_nfo(&text);
        if parsed.is_none() {
            tracing::debug!(path = %p.display(), "ignoring unreadable NFO");
        }
        parsed
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalArtwork {
    pub posters: Vec<String>,
    pub backdrops: Vec<String>,
}

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Local images as `file:` URLs: `poster.jpg`, `folder.jpg`, `<stem>-poster.jpg`, `fanart.jpg`, ...
pub fn find_local_artwork(dir: &Path, stem: Option<&str>) -> LocalArtwork {
    let find = |names: &[String]| -> Vec<String> {
        names
            .iter()
            .flat_map(|n| IMAGE_EXTS.iter().map(move |e| dir.join(format!("{n}.{e}"))))
            .filter(|p| p.is_file())
            .filter_map(|p| p.to_str().map(|s| format!("file:{s}")))
            .collect()
    };
    let with_stem = |suffix: &str, plain: &[&str]| -> Vec<String> {
        let mut names: Vec<String> = stem.map(|s| format!("{s}-{suffix}")).into_iter().collect();
        names.extend(plain.iter().map(|p| (*p).to_owned()));
        names
    };
    LocalArtwork {
        posters: find(&with_stem("poster", &["poster", "folder", "cover"])),
        backdrops: find(&with_stem("fanart", &["fanart", "backdrop", "background"])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_movie_nfo() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<movie>
  <title>Heat</title>
  <originaltitle>Heat</originaltitle>
  <year>1995</year>
  <plot>A group of professional bank robbers...</plot>
  <tagline>A Los Angeles crime saga</tagline>
  <rating>8.3</rating>
  <genre>Crime</genre>
  <genre>Thriller</genre>
  <studio>Warner Bros.</studio>
  <uniqueid type="tmdb" default="true">949</uniqueid>
  <uniqueid type="imdb">tt0113277</uniqueid>
  <director>Michael Mann</director>
  <actor><name>Al Pacino</name><role>Vincent Hanna</role><thumb>http://x/al.jpg</thumb></actor>
  <actor><name>Robert De Niro</name><role>Neil McCauley</role></actor>
</movie>"#;
        let b = parse_nfo(xml).unwrap();
        assert_eq!(b.title.as_deref(), Some("Heat"));
        assert_eq!(b.year, Some(1995));
        assert_eq!(b.tmdb_id, Some(949));
        assert_eq!(b.imdb_id.as_deref(), Some("tt0113277"));
        assert_eq!(b.genres, ["Crime", "Thriller"]);
        assert_eq!(b.rating, Some(8.3));
        assert_eq!(b.studio.as_deref(), Some("Warner Bros."));
        assert_eq!(b.people.len(), 3);
        assert_eq!(b.people[0].character.as_deref(), Some("Vincent Hanna"));
        assert_eq!(b.people[2].role, "director");
    }

    #[test]
    fn parses_tvshow_and_legacy_ids() {
        let xml = "<tvshow><title>Breaking Bad</title><premiered>2008-01-20</premiered>\
                   <tvdbid>81189</tvdbid><id>tt0903747</id></tvshow>";
        let b = parse_nfo(xml).unwrap();
        assert_eq!(b.year, Some(2008));
        assert_eq!(b.tvdb_id, Some(81189));
        assert_eq!(b.imdb_id.as_deref(), Some("tt0903747"));
    }

    #[test]
    fn non_xml_nfo_is_ignored() {
        assert!(parse_nfo("https://www.imdb.com/title/tt0113277/").is_none());
    }

    #[test]
    fn finds_local_artwork() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["poster.jpg", "Heat (1995)-fanart.png", "folder.jpg", "notes.txt"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        let art = find_local_artwork(dir.path(), Some("Heat (1995)"));
        assert_eq!(art.posters.len(), 2);
        assert!(art.posters[0].ends_with("poster.jpg"));
        assert_eq!(art.backdrops.len(), 1);
        assert!(art.backdrops[0].starts_with("file:"));
    }
}
