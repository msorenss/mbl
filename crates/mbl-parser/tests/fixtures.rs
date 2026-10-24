#![allow(clippy::unwrap_used, clippy::panic)]

use mbl_parser::{classify_extra, is_sample, parse_episode, parse_movie, parse_music};
use proptest::prelude::*;
use serde::Deserialize;

fn load<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

#[derive(Deserialize)]
struct Cases<T> {
    cases: Vec<T>,
}

#[derive(Deserialize)]
struct MovieCase {
    path: String,
    title: String,
    year: Option<i32>,
    tmdb: Option<i64>,
    tvdb: Option<i64>,
    imdb: Option<String>,
    edition: Option<String>,
    part: Option<u32>,
}

#[derive(Deserialize)]
struct EpisodeCase {
    path: String,
    show: String,
    year: Option<i32>,
    season: Option<u32>,
    #[serde(default)]
    episodes: Vec<u32>,
    absolute: Option<u32>,
    date: Option<String>,
    title: Option<String>,
    tmdb: Option<i64>,
    tvdb: Option<i64>,
    imdb: Option<String>,
}

#[derive(Deserialize)]
struct MusicCase {
    path: String,
    artist: Option<String>,
    album: Option<String>,
    album_year: Option<i32>,
    disc: Option<u32>,
    track: Option<u32>,
    title: Option<String>,
}

#[derive(Deserialize)]
struct ExtrasFile {
    extras: Vec<ExtraCase>,
    samples: Vec<SampleCase>,
}

#[derive(Deserialize)]
struct ExtraCase {
    path: String,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct SampleCase {
    path: String,
    sample: bool,
}

/// Collect every mismatch so a single run shows all failures.
fn report(kind: &str, failures: Vec<String>, total: usize) {
    assert!(
        failures.is_empty(),
        "{} of {total} {kind} cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn movies() {
    let cases: Cases<MovieCase> = load("movies.toml");
    let mut failures = Vec::new();
    for c in &cases.cases {
        let g = parse_movie(&c.path);
        let got = (&g.title, g.year, g.ids.tmdb, g.ids.tvdb, &g.ids.imdb, &g.edition, g.part);
        let want = (&c.title, c.year, c.tmdb, c.tvdb, &c.imdb, &c.edition, c.part);
        if got != want {
            failures.push(format!("{}\n    got  {got:?}\n    want {want:?}", c.path));
        }
    }
    report("movie", failures, cases.cases.len());
}

#[test]
fn episodes() {
    let cases: Cases<EpisodeCase> = load("episodes.toml");
    let mut failures = Vec::new();
    for c in &cases.cases {
        let g = parse_episode(&c.path);
        let date = g.air_date.map(|d| d.to_string());
        let got = (
            &g.show,
            g.show_year,
            g.season,
            &g.episodes,
            g.absolute,
            &date,
            &g.title,
            g.ids.tmdb,
            g.ids.tvdb,
            &g.ids.imdb,
        );
        let want = (
            &c.show,
            c.year,
            c.season,
            &c.episodes,
            c.absolute,
            &c.date,
            &c.title,
            c.tmdb,
            c.tvdb,
            &c.imdb,
        );
        if got != want {
            failures.push(format!("{}\n    got  {got:?}\n    want {want:?}", c.path));
        }
    }
    report("episode", failures, cases.cases.len());
}

#[test]
fn music() {
    let cases: Cases<MusicCase> = load("music.toml");
    let mut failures = Vec::new();
    for c in &cases.cases {
        let g = parse_music(&c.path);
        let got = (&g.artist, &g.album, g.album_year, g.disc, g.track, &g.title);
        let want = (&c.artist, &c.album, c.album_year, c.disc, c.track, &c.title);
        if got != want {
            failures.push(format!("{}\n    got  {got:?}\n    want {want:?}", c.path));
        }
    }
    report("music", failures, cases.cases.len());
}

#[test]
fn extras_and_samples() {
    let file: ExtrasFile = load("extras.toml");
    let mut failures = Vec::new();
    for c in &file.extras {
        let got = classify_extra(&c.path).map(|k| format!("{k:?}"));
        if got != c.kind {
            failures.push(format!("{}: got {got:?}, want {:?}", c.path, c.kind));
        }
    }
    for c in &file.samples {
        if is_sample(&c.path) != c.sample {
            failures.push(format!("{}: sample should be {}", c.path, c.sample));
        }
    }
    report("extra/sample", failures, file.extras.len() + file.samples.len());
}

#[test]
fn fixture_count_meets_target() {
    let total = load::<Cases<MovieCase>>("movies.toml").cases.len()
        + load::<Cases<EpisodeCase>>("episodes.toml").cases.len()
        + load::<Cases<MusicCase>>("music.toml").cases.len()
        + load::<ExtrasFile>("extras.toml").extras.len()
        + load::<ExtrasFile>("extras.toml").samples.len();
    assert!(total >= 300, "only {total} fixture cases");
}

proptest! {
    #[test]
    fn never_panics(path in "\\PC{0,200}") {
        let _ = parse_movie(&path);
        let _ = parse_episode(&path);
        let _ = parse_music(&path);
        let _ = classify_extra(&path);
        let _ = is_sample(&path);
    }

    #[test]
    fn never_panics_on_pathlike(path in "([A-Za-z0-9 ._()\\[\\]{}-]{0,30}/){0,4}[A-Za-z0-9 ._()\\[\\]{}-]{0,60}") {
        let _ = parse_movie(&path);
        let _ = parse_episode(&path);
        let _ = parse_music(&path);
    }
}
