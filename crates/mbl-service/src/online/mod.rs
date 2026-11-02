//! Internet title discovery and bounded public-source inspection, independent of private libraries.
mod hls;
mod http;
mod metadata;
mod probe;
pub(crate) mod search;
mod session;
#[cfg(test)]
mod tests;

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mbl_core::online::{OnlineAvailability as Availability, OnlineSearchResponse, OnlineSource};
use mbl_core::{Actor, Error, LibraryAccess, Result};
use mbl_db::{Db, repo::online::OnlineRepo};
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Semaphore};

use self::http::{Fetched, OnlineHttp, PublicHttp, public_url};
use crate::PluginService;

const SEARCH_URL: &str = "https://api.mwmbl.org/api/v2/search/";
const MAX_SAMPLE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct Hit {
    url: String,
    title: String,
    #[serde(default)]
    content: String,
}
#[derive(Deserialize)]
struct SearchDocument {
    results: Vec<Hit>,
}
#[derive(Debug, Clone)]
struct CachedSearch {
    query: String,
    at: Instant,
    hits: Vec<Hit>,
}

#[derive(Debug, Clone)]
pub struct OnlineService {
    repo: OnlineRepo,
    http: Arc<dyn OnlineHttp>,
    searches: Arc<Mutex<VecDeque<CachedSearch>>>,
    last_search: Arc<Mutex<Option<Instant>>>,
    inspections: Arc<Semaphore>,
    plugins: PluginService,
    tools: Option<mbl_probe::FfTools>,
    db: Db,
    sessions: Arc<Mutex<std::collections::HashMap<String, session::Session>>>,
    transfers: Arc<Semaphore>,
}

impl OnlineService {
    pub fn new(db: Db) -> Self {
        Self {
            db: db.clone(),
            sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
            transfers: Arc::new(Semaphore::new(4)),
            repo: OnlineRepo::new(db.clone()),
            tools: None,
            plugins: PluginService::new(Arc::new(db)),
            http: Arc::new(PublicHttp),
            searches: Arc::new(Mutex::new(VecDeque::new())),
            last_search: Arc::new(Mutex::new(None)),
            inspections: Arc::new(Semaphore::new(2)),
        }
    }

    pub fn with_tools(mut self, tools: Option<mbl_probe::FfTools>) -> Self {
        self.tools = tools;
        self
    }

    fn authorize(actor: &Actor) -> Result<()> {
        // Public web results have no trustworthy parental rating or local-library ACL.
        if actor.is_admin
            || (!actor.is_managed
                && actor.content_rating_ceiling.is_none()
                && actor.library_access == LibraryAccess::All)
        {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    pub async fn search(&self, actor: &Actor, raw: &str) -> Result<OnlineSearchResponse> {
        Self::authorize(actor)?;
        self.require_enabled().await?;
        let query = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if !(2..=160).contains(&query.chars().count()) || query.chars().any(char::is_control) {
            return Err(Error::Validation("Search title must contain 2–160 characters".into()));
        }
        let config = self.plugins.get_config("webdrama").await?;
        let (provider, endpoint) = search::provider(&config.settings)?;
        let key = format!("{} {}", endpoint, query.to_lowercase());
        let mut personal: Vec<_> = self
            .repo
            .recent(actor.user_id)
            .await?
            .into_iter()
            .filter(|s| search::matches(s, &query))
            .map(fresh)
            .collect();
        personal.sort_by_key(|s| std::cmp::Reverse(search::rank(s, &query)));
        let remote = async {
            let cached = self
                .searches
                .lock()
                .await
                .iter()
                .find(|c| c.query == key && c.at.elapsed() < Duration::from_secs(600))
                .map(|c| c.hits.clone());
            let hits = if let Some(hits) = cached {
                tracing::debug!(target: "mbl::online", query = %query, hits = hits.len(), "returning cached search results");
                hits
            } else {
                let mut gate = self.last_search.lock().await;
                if gate.is_some_and(|last| last.elapsed() < Duration::from_secs(1)) {
                    tracing::warn!(target: "mbl::online", "search rate limit reached (1s throttle)");
                    return Err(Error::RateLimited { retry_after_secs: 1 });
                }
                *gate = Some(Instant::now());
                drop(gate);
                let mut url = endpoint.clone();
                if provider == "SearXNG" {
                    url.query_pairs_mut()
                        .append_pair("q", &format!("{query} watch online"))
                        .append_pair("format", "json")
                        .append_pair("categories", "general,videos");
                } else {
                    url.query_pairs_mut().append_pair("q", &query);
                }
                tracing::info!(target: "mbl::online", provider, query = %query, "querying search provider");
                let fetch_start = Instant::now();
                let response = match tokio::time::timeout(
                    Duration::from_secs(12),
                    self.http.fetch(url, 1024 * 1024, false),
                )
                .await
                {
                    Ok(Ok(res)) => res,
                    Ok(Err(e)) => {
                        tracing::warn!(target: "mbl::online", provider, error = %e, elapsed_ms = fetch_start.elapsed().as_millis(), "search provider request failed");
                        return Err(e);
                    }
                    Err(_) => {
                        tracing::warn!(target: "mbl::online", provider, elapsed_ms = fetch_start.elapsed().as_millis(), "search provider request timed out");
                        return Err(Error::Unavailable("Internet search timed out".into()));
                    }
                };
                if response.status == 429 {
                    tracing::warn!(target: "mbl::online", provider, "search provider returned HTTP 429 rate limit");
                    return Err(Error::RateLimited { retry_after_secs: 60 });
                }
                if response.status != 200 {
                    tracing::warn!(target: "mbl::online", provider, status = response.status, "search provider returned non-200 status");
                    return Err(Error::Unavailable(
                        "Internet search provider is unavailable".into(),
                    ));
                }
                let doc: SearchDocument = serde_json::from_slice(&response.body).map_err(|e| {
                    tracing::warn!(target: "mbl::online", provider, error = %e, "search provider returned invalid JSON response");
                    Error::Unavailable("Search provider returned an invalid response".into())
                })?;
                let hits: Vec<_> = doc.results.into_iter().take(40).collect();
                tracing::info!(target: "mbl::online", provider, hits = hits.len(), elapsed_ms = fetch_start.elapsed().as_millis(), "received hits from search provider");
                let mut cache = self.searches.lock().await;
                cache.push_front(CachedSearch {
                    query: key.clone(),
                    at: Instant::now(),
                    hits: hits.clone(),
                });
                cache.truncate(32);
                hits
            };
            Ok::<_, Error>(hits)
        }
        .await;
        let (hits, warning) = match remote {
            Ok(hits) => (hits, None),
            Err(error) if !personal.is_empty() => {
                tracing::warn!(target: "mbl::online", error = %error, "remote search failed; falling back to indexed personal sources");
                (
                    Vec::new(),
                    Some(format!(
                        "Internet search unavailable: {error}. Showing your indexed sources."
                    )),
                )
            }
            Err(error) => return Err(error),
        };
        let mut seen = HashSet::new();
        let mut results = Vec::new();
        for hit in hits {
            let Ok(url) = public_url(&hit.url) else {
                continue;
            };
            if !seen.insert(url.to_string()) {
                continue;
            }
            let mut item = source(url.as_str());
            item.title = metadata::text(&hit.title, 300);
            if item.title.is_empty() {
                item.title = url.host_str().unwrap_or("Source").into();
            }
            item.summary = (!hit.content.is_empty()).then(|| metadata::text(&hit.content, 1000));
            if let Some(indexed) = self.repo.get(actor.user_id, &item.id).await? {
                item = fresh(indexed);
            }
            results.push(item);
        }
        results.sort_by_key(|s| std::cmp::Reverse(search::rank(s, &query)));
        results.truncate(20);
        let visible: HashSet<_> = results.iter().map(|s| s.id.as_str()).collect();
        let indexed =
            personal.into_iter().filter(|s| !visible.contains(s.id.as_str())).take(20).collect();
        self.require_enabled().await?;
        Ok(OnlineSearchResponse { query, provider: provider.into(), results, indexed, warning })
    }

    pub async fn inspect(&self, actor: &Actor, raw_url: &str) -> Result<OnlineSource> {
        Self::authorize(actor)?;
        self.require_enabled().await?;
        let url = public_url(raw_url)?;
        let _permit = self
            .inspections
            .try_acquire()
            .map_err(|_| Error::RateLimited { retry_after_secs: 1 })?;
        let mut source = source(url.as_str());
        source.checked_at = Some(mbl_core::time::now_ms());
        tracing::info!(target: "mbl::online", url = %url, "starting online source inspection");
        let start = Instant::now();
        match tokio::time::timeout(Duration::from_secs(18), self.inspect_source(&mut source, url))
            .await
        {
            Ok(Err(error @ Error::Validation(_))) => return Err(error),
            Ok(Err(error)) => {
                tracing::warn!(target: "mbl::online", url = %source.url, error = %error, elapsed_ms = start.elapsed().as_millis(), "inspection failed with error");
                source.availability = Availability::Unavailable;
                source.detail = "The public source could not be checked. Try again later.".into();
            }
            Err(_) => {
                tracing::warn!(target: "mbl::online", url = %source.url, elapsed_ms = start.elapsed().as_millis(), "inspection timed out (18s)");
                source.availability = Availability::Unknown;
                source.detail =
                    "The source check timed out. Playback has not been verified.".into();
            }
            Ok(Ok(())) => {
                tracing::info!(
                    target: "mbl::online",
                    url = %source.url,
                    availability = ?source.availability,
                    has_media = source.media_url.is_some(),
                    elapsed_ms = start.elapsed().as_millis(),
                    "inspection completed successfully"
                );
            }
        }
        self.require_enabled().await?;
        while !source.episodes.is_empty()
            && serde_json::to_vec(&source).map_err(Error::internal)?.len() > 64 * 1024
        {
            source.episodes.pop();
        }
        self.repo.save(actor.user_id, &source).await?;
        Ok(source)
    }

    async fn require_enabled(&self) -> Result<()> {
        if self.plugins.get_config("webdrama").await?.enabled {
            Ok(())
        } else {
            Err(Error::Unavailable(
                "Enable WebDrama before searching or checking public sources".into(),
            ))
        }
    }

    async fn inspect_source(&self, source: &mut OnlineSource, url: Url) -> Result<()> {
        let response = self.http.fetch(url, MAX_SAMPLE, true).await?;
        if !status(source, &response) {
            return Ok(());
        }
        if is_html(&response) {
            let html = String::from_utf8_lossy(&response.body);
            source.availability = Availability::ExternalPage;
            source.detail = "Page found. No directly verifiable video source was exposed; open the website to continue.".into();
            if let Some(media) = metadata::extract(source, &response.url, &html) {
                let media = self.http.fetch(public_url(&media)?, MAX_SAMPLE, true).await?;
                return self.check_media(source, media).await;
            }
            if matches!(source.kind.as_str(), "video" | "episode" | "movie")
                || html.contains("og:video")
                || html.contains("__initialState")
            {
                source.has_video = Some(true);
                source.detail = "Video player detected on website. The source uses client-side playback or protection; open the website to stream.".into();
            }
            Ok(())
        } else {
            self.check_media(source, response).await
        }
    }

    async fn check_media(&self, source: &mut OnlineSource, response: Fetched) -> Result<()> {
        if !status(source, &response) {
            return Ok(());
        }
        if response.body.starts_with(b"#EXTM3U") || response.content_type.contains("mpegurl") {
            source.availability = Availability::ExternalPlayer;
            source.detail = "An HLS source was found. The online player can check supported unencrypted VOD before relaying it.".into();
        } else if is_html(&response) {
            source.availability = Availability::ExternalPage;
            source.detail =
                "The video link returned a web page, not verified media. Open the source website."
                    .into();
        } else {
            match probe::media_sample(&response.body, self.tools.as_ref()).await {
                Ok(has_video) => {
                    if source.kind == "page" {
                        source.kind = if has_video { "video" } else { "audio" }.into();
                    }
                    source.availability = Availability::MediaDetected;
                    source.media_url = Some(response.url.to_string());
                    source.has_video = Some(has_video);
                    source.detail = "A short media sample decoded successfully. Preview tests whether this browser can play it.".into();
                }
                Err(error) => {
                    source.availability = Availability::Unknown;
                    source.detail = match error { Error::Unavailable(message) => message, _ => "The limited sample could not confirm playable video or audio. Open the website or retry.".into() };
                }
            }
        }
        Ok(())
    }
}

fn status(source: &mut OnlineSource, response: &Fetched) -> bool {
    if response.status == 401 || response.status == 403 {
        source.availability = Availability::Restricted;
        source.detail =
            "The website denied automated access or requires sign-in. Open it directly.".into();
        false
    } else if !(200..300).contains(&response.status) {
        source.availability = Availability::Unavailable;
        source.detail =
            format!("Source returned HTTP {}. No playback was verified.", response.status);
        false
    } else {
        true
    }
}
fn is_html(response: &Fetched) -> bool {
    response.content_type.contains("html")
        || response.body.iter().position(|b| !b.is_ascii_whitespace()).is_some_and(|p| {
            response.body[p..].starts_with(b"<!DOCTYPE")
                || response.body[p..].starts_with(b"<!doctype")
                || response.body[p..].starts_with(b"<html")
        })
}
fn source(url: &str) -> OnlineSource {
    OnlineSource {
        id: Sha256::digest(url.as_bytes()).iter().map(|b| format!("{b:02x}")).collect(),
        url: url.into(),
        title: url.into(),
        summary: None,
        kind: "page".into(),
        series_title: None,
        season_number: None,
        episode_number: None,
        episodes: Vec::new(),
        availability: Availability::Unchecked,
        checked_at: None,
        media_url: None,
        has_video: None,
        detail: "Search result; playback has not been checked.".into(),
    }
}
fn fresh(mut source: OnlineSource) -> OnlineSource {
    let age = source.checked_at.map(|at| mbl_core::time::now_ms().saturating_sub(at));
    if !age.is_some_and(|age| (0..=300_000).contains(&age)) {
        source.availability = Availability::Unchecked;
        source.media_url = None;
        source.has_video = None;
        source.detail = "This source needs a fresh playback check.".into();
    }
    source
}
