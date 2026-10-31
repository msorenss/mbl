use std::collections::HashSet;
use std::sync::LazyLock;

use mbl_core::online::{OnlineEpisode, OnlineSource};
use regex::Regex;
use reqwest::Url;
use serde_json::Value;

use super::http::public_url;

static SCRIPTS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?is)<script\b([^>]*)>(.*?)</script\s*>").ok());
static JSON_TYPE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:^|\s)type\s*=\s*(?:"application/ld\+json"|'application/ld\+json'|application/ld\+json(?:\s|$))"#).ok()
});
static TITLE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?is)<title\b[^>]*>([^<]{0,1000})</title\s*>").ok());

pub(super) fn text(raw: &str, max: usize) -> String {
    raw.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
        .split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control() && !matches!(c,'\u{200e}'|'\u{200f}'|'\u{061c}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')).take(max).collect()
}

fn name(node: &Value) -> Option<String> {
    node.get("name").and_then(Value::as_str).map(|s| text(s, 300)).filter(|s| !s.is_empty())
}
fn number(value: Option<&Value>) -> Option<u32> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v <= 100_000)
}
fn has_type(node: &Value, kind: &str) -> bool {
    let matches = |v: &Value| v.as_str().is_some_and(|s| s.rsplit('/').next() == Some(kind));
    node.get("@type")
        .is_some_and(|t| matches(t) || t.as_array().is_some_and(|a| a.iter().any(matches)))
}
fn resolve(base: &Url, raw: &str) -> Option<String> {
    let url = base.join(raw).ok()?;
    Some(public_url(url.as_str()).ok()?.to_string())
}

struct Extract<'a> {
    source: &'a mut OnlineSource,
    base: &'a Url,
    media: Option<String>,
    budget: usize,
    root_rank: u8,
}
impl Extract<'_> {
    fn walk(
        &mut self,
        node: &Value,
        depth: u8,
        inherited_series: Option<&str>,
        inherited_season: Option<u32>,
        media_allowed: bool,
    ) {
        if depth > 10 || self.budget == 0 {
            return;
        }
        self.budget -= 1;
        if let Some(array) = node.as_array() {
            for child in array.iter().take(64) {
                self.walk(child, depth + 1, inherited_series, inherited_season, media_allowed);
            }
            return;
        }
        if !node.is_object() {
            return;
        }
        let series = node
            .get("partOfSeries")
            .and_then(name)
            .or_else(|| if has_type(node, "TVSeries") { name(node) } else { None })
            .or_else(|| inherited_series.map(str::to_owned));
        let season = number(node.get("seasonNumber"))
            .or_else(|| number(node.get("partOfSeason").and_then(|s| s.get("seasonNumber"))))
            .or(inherited_season);
        let (kind, rank) = if has_type(node, "TVEpisode") {
            ("episode", 5)
        } else if has_type(node, "TVSeason") {
            ("season", 4)
        } else if has_type(node, "TVSeries") {
            ("series", 3)
        } else if has_type(node, "Movie") {
            ("movie", 2)
        } else if has_type(node, "VideoObject") {
            ("video", 1)
        } else {
            ("page", 0)
        };
        // A series document stays a series even when it lists its child episodes.
        if rank > 0 && self.root_rank == 0 {
            self.root_rank = rank;
            self.source.kind = kind.into();
            if let Some(title) = name(node) {
                self.source.title = title;
            }
            self.source.summary = node
                .get("description")
                .and_then(Value::as_str)
                .map(|s| text(s, 1000))
                .or(self.source.summary.take());
            self.source.series_title = series.clone();
            self.source.season_number = season;
            self.source.episode_number = number(node.get("episodeNumber"));
        }
        if kind == "episode"
            && matches!(self.source.kind.as_str(), "series" | "season")
            && self.source.episodes.len() < 64
        {
            let raw_url = node
                .get("url")
                .and_then(Value::as_str)
                .or_else(|| node.get("@id").and_then(Value::as_str));
            if let Some(url) = raw_url.and_then(|raw| resolve(self.base, raw))
                && !self.source.episodes.iter().any(|e| e.url == url)
            {
                self.source.episodes.push(OnlineEpisode {
                    title: name(node).unwrap_or_else(|| "Episode".into()),
                    url,
                    season_number: season,
                    episode_number: number(node.get("episodeNumber")),
                });
            }
        }
        // Only a root media object (or its own associated media) supplies preview media.
        // A series page listing episode videos must not preview an arbitrary first episode.
        if media_allowed
            && self.media.is_none()
            && matches!(self.source.kind.as_str(), "video" | "movie" | "episode")
        {
            self.media =
                node.get("contentUrl").and_then(Value::as_str).and_then(|s| resolve(self.base, s));
        }
        for key in [
            "@graph",
            "mainEntity",
            "hasPart",
            "episode",
            "episodes",
            "containsSeason",
            "season",
            "video",
            "associatedMedia",
            "encoding",
        ] {
            if let Some(child) = node.get(key) {
                self.walk(
                    child,
                    depth + 1,
                    series.as_deref(),
                    season,
                    media_allowed && matches!(key, "video" | "associatedMedia" | "encoding"),
                );
            }
        }
    }
}

pub(super) fn extract(source: &mut OnlineSource, base: &Url, html: &str) -> Option<String> {
    let cleaned = COMMENTS.as_ref().map(|regex| regex.replace_all(html, ""));
    let html = cleaned.as_deref().unwrap_or(html);
    if let Some(regex) = TITLE.as_ref()
        && let Some(title) = regex.captures(html).and_then(|c| c.get(1))
    {
        let title = text(title.as_str(), 300);
        if !title.is_empty() {
            source.title = title;
        }
    }
    let (Some(scripts), Some(json_type)) = (SCRIPTS.as_ref(), JSON_TYPE.as_ref()) else {
        return None;
    };
    let documents: Vec<Value> = scripts
        .captures_iter(html)
        .take(24)
        .filter(|capture| json_type.is_match(&capture[1]) && capture[2].len() <= 512 * 1024)
        .filter_map(|capture| serde_json::from_str(&capture[2]).ok())
        .collect();
    let mut references = HashSet::new();
    let mut budget = 512;
    for doc in &documents {
        main_references(doc, base, &mut references, &mut budget, 0);
    }
    let mut primary = Primary { base, selected: None, score: 0, budget: 512, references };
    for doc in &documents {
        primary.visit(doc, 0, true, false);
    }
    let mut extract = Extract { source, base, media: None, budget: 512, root_rank: 0 };
    if let Some(node) = primary.selected {
        extract.walk(node, 0, None, None, true);
    }
    extract.source.episodes.sort_by_key(|e| (e.season_number, e.episode_number));
    // Structured series/season pages must not inherit a random embedded trailer.
    if extract.media.is_none() && !matches!(extract.source.kind.as_str(), "series" | "season") {
        extract.media = html_media(base, html);
    }
    extract.media
}

struct Primary<'a> {
    base: &'a Url,
    selected: Option<&'a Value>,
    score: u16,
    budget: usize,
    references: HashSet<String>,
}
impl<'a> Primary<'a> {
    fn visit(&mut self, node: &'a Value, depth: u8, eligible: bool, main: bool) {
        if depth > 10 || self.budget == 0 {
            return;
        }
        self.budget -= 1;
        if let Some(items) = node.as_array() {
            for item in items.iter().take(64) {
                self.visit(item, depth + 1, eligible, main);
            }
            return;
        }
        let rank = ["VideoObject", "Movie", "TVSeason", "TVSeries", "TVEpisode"]
            .iter()
            .position(|kind| has_type(node, kind))
            .map(|i| i as u16 + 1)
            .unwrap_or(0);
        // Fragment IDs identify graph nodes, not the page itself.
        let same_page = node
            .get("url")
            .and_then(Value::as_str)
            .and_then(|url| resolve(self.base, url))
            .is_some_and(|url| url == self.base.as_str());
        let referenced = node
            .get("@id")
            .and_then(Value::as_str)
            .and_then(|id| self.base.join(id).ok())
            .is_some_and(|id| self.references.contains(id.as_str()));
        let score = rank
            + if main || referenced {
                300
            } else if same_page {
                200
            } else {
                0
            };
        if rank > 0 && (eligible || same_page || referenced) && score > self.score {
            self.selected = Some(node);
            self.score = score;
        }
        for key in [
            "@graph",
            "mainEntity",
            "hasPart",
            "episode",
            "episodes",
            "containsSeason",
            "season",
            "video",
            "associatedMedia",
            "encoding",
        ] {
            if let Some(child) = node.get(key) {
                self.visit(
                    child,
                    depth + 1,
                    eligible && matches!(key, "@graph" | "mainEntity"),
                    key == "mainEntity",
                );
            }
        }
    }
}

static HTML_TAGS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?is)<(/?)([a-z][a-z0-9:-]*)\b((?:"[^"]*"|'[^']*'|[^'">])*)>"#).ok()
});
static SRC: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"(?is)(?:^|\s)src\s*=\s*(?:"([^"]*)"|'([^']*)')"#).ok());
static COMMENTS: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").ok());

/// Conservative HTML fallback: one video, one unique quoted source, no scripts/iframes.
fn html_media(base: &Url, html: &str) -> Option<String> {
    let html = COMMENTS.as_ref()?.replace_all(html, "");
    let tags = HTML_TAGS.as_ref()?;
    let src = SRC.as_ref()?;
    let mut skip: Option<String> = None;
    let mut template_depth = 0usize;
    let mut videos = 0usize;
    let mut inside = false;
    let mut candidates = Vec::new();
    for tag in tags.captures_iter(&html).take(8192) {
        let name = tag[2].to_ascii_lowercase();
        let end = &tag[1] == "/";
        if let Some(raw) = &skip {
            if end && &name == raw {
                skip = None;
            }
            continue;
        }
        if !end && matches!(name.as_str(), "script" | "style" | "noscript") {
            skip = Some(name);
            continue;
        }
        if name == "template" {
            if end {
                template_depth = template_depth.saturating_sub(1);
            } else {
                template_depth += 1;
            }
            continue;
        }
        if template_depth > 0 {
            continue;
        }
        if name == "video" {
            if end {
                inside = false;
                continue;
            }
            videos += 1;
            inside = true;
        }
        if inside
            && !end
            && matches!(name.as_str(), "video" | "source")
            && let Some(value) = src.captures(&tag[3]).and_then(|c| c.get(1).or_else(|| c.get(2)))
        {
            let raw = value.as_str().replace("&amp;", "&");
            // Any invalid candidate makes this ambiguous, rather than choosing another video.
            let url = resolve(base, &raw)?;
            if !candidates.contains(&url) {
                candidates.push(url);
            }
        }
    }
    if videos == 1 && candidates.len() == 1 { candidates.pop() } else { None }
}

fn main_references(
    node: &Value,
    base: &Url,
    ids: &mut HashSet<String>,
    budget: &mut usize,
    depth: u8,
) {
    if *budget == 0 || depth > 10 {
        return;
    }
    *budget -= 1;
    if let Some(items) = node.as_array() {
        for item in items.iter().take(64) {
            main_references(item, base, ids, budget, depth + 1);
        }
    }
    if has_type(node, "WebPage")
        && node
            .get("url")
            .and_then(Value::as_str)
            .is_none_or(|url| resolve(base, url).is_some_and(|url| url == base.as_str()))
        && let Some(id) = node
            .get("mainEntity")
            .and_then(|entity| entity.get("@id"))
            .and_then(Value::as_str)
            .and_then(|id| base.join(id).ok())
    {
        ids.insert(id.to_string());
    }
    for key in ["@graph", "mainEntity"] {
        if let Some(child) = node.get(key) {
            main_references(child, base, ids, budget, depth + 1);
        }
    }
}
