//! TMDB v3 client with rate limiting and an on-disk response cache.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use mbl_core::{Error, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::{Candidate, Credit, MetadataBundle};

const IMAGE_BASE: &str = "https://image.tmdb.org/t/p/original";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// TMDB allows ~40 requests per 10 seconds.
const BURST: NonZeroU32 = NonZeroU32::MIN.saturating_add(39);
const MAX_CAST: usize = 20;

#[derive(Debug, Clone)]
pub struct TmdbConfig {
    /// v3 API key, or a v4 read access token (sent as a bearer token).
    pub api_key: String,
    pub base_url: String,
    pub language: String,
    /// Directory for cached responses; `None` disables caching.
    pub cache_dir: Option<PathBuf>,
}

impl TmdbConfig {
    pub fn new(api_key: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.themoviedb.org/3".into(),
            language: language.into(),
            cache_dir: None,
        }
    }
}

#[derive(Clone)]
pub struct TmdbClient {
    http: reqwest::Client,
    config: Arc<TmdbConfig>,
    limiter: Arc<DefaultDirectRateLimiter>,
}

impl std::fmt::Debug for TmdbClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TmdbClient")
            .field("base_url", &self.config.base_url)
            .finish_non_exhaustive()
    }
}

/// One episode from a TMDB season.
#[derive(Debug, Clone, PartialEq)]
pub struct TmdbEpisode {
    pub number: u32,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub air_date: Option<String>,
    pub rating: Option<f64>,
    pub still: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TmdbSeason {
    pub summary: Option<String>,
    pub poster: Option<String>,
    pub episodes: Vec<TmdbEpisode>,
}

impl TmdbClient {
    pub fn new(config: TmdbConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("MBL/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(Error::internal)?;
        let quota = Quota::with_period(Duration::from_millis(250))
            .unwrap_or_else(|| Quota::per_second(NonZeroU32::MIN))
            .allow_burst(BURST);
        Ok(Self { http, config: Arc::new(config), limiter: Arc::new(RateLimiter::direct(quota)) })
    }

    pub async fn search_movie(&self, title: &str, year: Option<i32>) -> Result<Vec<Candidate>> {
        let mut query = vec![("query", title.to_owned())];
        if let Some(y) = year {
            query.push(("year", y.to_string()));
        }
        let res: SearchResults = self.get("/search/movie", &query).await?;
        Ok(res.results.into_iter().map(|r| r.candidate()).collect())
    }

    pub async fn search_tv(&self, title: &str, year: Option<i32>) -> Result<Vec<Candidate>> {
        let mut query = vec![("query", title.to_owned())];
        if let Some(y) = year {
            query.push(("first_air_date_year", y.to_string()));
        }
        let res: SearchResults = self.get("/search/tv", &query).await?;
        Ok(res.results.into_iter().map(|r| r.candidate()).collect())
    }

    pub async fn movie(&self, id: i64) -> Result<MetadataBundle> {
        let d: Details = self
            .get(&format!("/movie/{id}"), &[("append_to_response", "credits".to_owned())])
            .await?;
        Ok(d.bundle(id))
    }

    pub async fn tv(&self, id: i64) -> Result<MetadataBundle> {
        let d: Details = self
            .get(&format!("/tv/{id}"), &[("append_to_response", "credits,external_ids".to_owned())])
            .await?;
        Ok(d.bundle(id))
    }

    pub async fn season(&self, tv_id: i64, season: u32) -> Result<TmdbSeason> {
        let s: Season = self.get(&format!("/tv/{tv_id}/season/{season}"), &[]).await?;
        Ok(TmdbSeason {
            summary: non_empty(s.overview),
            poster: image(s.poster_path),
            episodes: s
                .episodes
                .into_iter()
                .map(|e| TmdbEpisode {
                    number: e.episode_number,
                    title: non_empty(e.name),
                    summary: non_empty(e.overview),
                    air_date: non_empty(e.air_date),
                    rating: e.vote_average.filter(|r| *r > 0.0),
                    still: image(e.still_path),
                })
                .collect(),
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let mut params: Vec<(&str, String)> = query.to_vec();
        params.push(("language", self.config.language.clone()));
        let cache_key = format!("{path}?{}", serde_json::to_string(&params).unwrap_or_default());

        if let Some(body) = self.cache_read(&cache_key)
            && let Ok(v) = serde_json::from_slice(&body)
        {
            return Ok(v);
        }

        // v4 read access tokens are JWTs; v3 keys are 32 hex chars.
        let bearer = self.config.api_key.len() > 40;
        if !bearer {
            params.push(("api_key", self.config.api_key.clone()));
        }
        let url = format!("{}{path}", self.config.base_url.trim_end_matches('/'));
        let mut attempt = 0u32;
        let body = loop {
            self.limiter.until_ready().await;
            let mut req = self.http.get(&url).query(&params);
            if bearer {
                req = req.bearer_auth(&self.config.api_key);
            }
            let res = req
                .send()
                .await
                .map_err(|e| Error::External(format!("TMDB request failed: {e}")))?;
            let status = res.status();
            if status.as_u16() == 429 || status.is_server_error() {
                attempt += 1;
                if attempt > 3 {
                    return Err(Error::External(format!("TMDB returned {status}")));
                }
                tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
                continue;
            }
            if status.as_u16() == 404 {
                return Err(Error::NotFound);
            }
            if status.as_u16() == 401 {
                return Err(Error::External("TMDB rejected the API key".into()));
            }
            if !status.is_success() {
                return Err(Error::External(format!("TMDB returned {status}")));
            }
            break res.bytes().await.map_err(|e| Error::External(e.to_string()))?;
        };
        let parsed = serde_json::from_slice(&body)
            .map_err(|e| Error::External(format!("unexpected TMDB response: {e}")))?;
        self.cache_write(&cache_key, &body);
        Ok(parsed)
    }

    fn cache_path(&self, key: &str) -> Option<PathBuf> {
        let dir = self.config.cache_dir.as_ref()?;
        let hash = Sha256::digest(key.as_bytes());
        let name: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        Some(dir.join(format!("{name}.json")))
    }

    fn cache_read(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.cache_path(key)?;
        let age = std::fs::metadata(&path).ok()?.modified().ok()?;
        (SystemTime::now().duration_since(age).unwrap_or_default() < CACHE_TTL)
            .then(|| std::fs::read(&path).ok())
            .flatten()
    }

    fn cache_write(&self, key: &str, body: &[u8]) {
        let Some(path) = self.cache_path(key) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, body) {
            tracing::debug!(error = %e, "could not write TMDB cache");
        }
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

fn image(path: Option<String>) -> Option<String> {
    non_empty(path).map(|p| format!("{IMAGE_BASE}{p}"))
}

fn year_of(date: Option<&str>) -> Option<i32> {
    date?.get(..4)?.parse().ok()
}

#[derive(Deserialize)]
struct SearchResults {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    id: i64,
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
}

impl SearchResult {
    fn candidate(self) -> Candidate {
        Candidate {
            tmdb_id: self.id,
            title: self.title.or(self.name).unwrap_or_default(),
            original_title: self.original_title.or(self.original_name),
            year: year_of(self.release_date.as_deref().or(self.first_air_date.as_deref())),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Named {
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Details {
    title: Option<String>,
    name: Option<String>,
    original_title: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    vote_average: Option<f64>,
    genres: Vec<Named>,
    production_companies: Vec<Named>,
    networks: Vec<Named>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    imdb_id: Option<String>,
    external_ids: ExternalIds,
    credits: Credits,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Credits {
    cast: Vec<Person>,
    crew: Vec<Person>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Person {
    id: i64,
    name: String,
    character: Option<String>,
    job: Option<String>,
    profile_path: Option<String>,
}

impl Person {
    fn credit(self, role: &str) -> Credit {
        Credit {
            name: self.name,
            tmdb_id: Some(self.id),
            thumb_url: image(self.profile_path),
            role: role.into(),
            character: non_empty(self.character),
        }
    }
}

impl Details {
    fn bundle(self, id: i64) -> MetadataBundle {
        let date = non_empty(self.release_date.or(self.first_air_date));
        let mut people: Vec<Credit> =
            self.credits.cast.into_iter().take(MAX_CAST).map(|p| p.credit("actor")).collect();
        for p in self.credits.crew {
            let role = match p.job.as_deref() {
                Some("Director") => "director",
                Some("Screenplay" | "Writer") => "writer",
                _ => continue,
            };
            people.push(p.credit(role));
        }
        MetadataBundle {
            title: non_empty(self.title.or(self.name)),
            original_title: non_empty(self.original_title.or(self.original_name)),
            summary: non_empty(self.overview),
            tagline: non_empty(self.tagline),
            year: year_of(date.as_deref()),
            release_date: date,
            rating: self.vote_average.filter(|r| *r > 0.0),
            studio: self
                .production_companies
                .into_iter()
                .chain(self.networks)
                .map(|n| n.name)
                .find(|n| !n.is_empty()),
            genres: self.genres.into_iter().map(|g| g.name).filter(|g| !g.is_empty()).collect(),
            people,
            posters: image(self.poster_path).into_iter().collect(),
            backdrops: image(self.backdrop_path).into_iter().collect(),
            tmdb_id: Some(id),
            tvdb_id: self.external_ids.tvdb_id,
            imdb_id: non_empty(self.imdb_id.or(self.external_ids.imdb_id)),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Season {
    overview: Option<String>,
    poster_path: Option<String>,
    episodes: Vec<Episode>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Episode {
    episode_number: u32,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    vote_average: Option<f64>,
    still_path: Option<String>,
}
