//! MusicBrainz metadata client with strict 1 req/s rate limiting and Cover Art Archive integration.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use mbl_core::{Error, Result};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};

const MB_BASE: &str = "https://musicbrainz.org/ws/2";
const CAA_BASE: &str = "https://coverartarchive.org";
const USER_AGENT_VAL: &str = "MBL/0.6.0 ( https://github.com/msorenss/mbl )";

pub type DirectRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MbArtist {
    pub id: String,
    pub name: String,
    pub disambiguation: Option<String>,
    pub country: Option<String>,
    pub score: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MbRelease {
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub country: Option<String>,
    pub artist_credit: Option<String>,
    pub track_count: Option<u32>,
    pub score: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MbReleaseDetails {
    pub id: String,
    pub title: String,
    pub date: Option<String>,
    pub artist: String,
    pub tracks: Vec<MbTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MbTrack {
    pub id: String,
    pub title: String,
    pub position: u32,
    pub duration_ms: Option<u32>,
}

#[derive(Clone)]
pub struct MusicBrainzClient {
    client: reqwest::Client,
    limiter: Arc<DirectRateLimiter>,
    base_url: String,
}

impl MusicBrainzClient {
    pub fn new() -> Self {
        Self::with_base_url(MB_BASE)
    }

    pub fn with_base_url(base_url: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VAL));
        headers.insert(reqwest::header::ACCEPT, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        // MusicBrainz strictly requires maximum 1 request per second
        let quota = Quota::with_period(Duration::from_secs(1))
            .unwrap_or_else(|| Quota::per_second(NonZeroU32::MIN));
        let limiter = Arc::new(RateLimiter::direct(quota));

        Self { client, limiter, base_url: base_url.trim_end_matches('/').to_owned() }
    }

    async fn wait_rate_limit(&self) {
        self.limiter.until_ready().await;
    }

    /// Search for an artist by name.
    pub async fn search_artist(&self, query: &str) -> Result<Vec<MbArtist>> {
        self.wait_rate_limit().await;
        let url = format!("{}/artist", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("query", query), ("fmt", "json")])
            .send()
            .await
            .map_err(Error::internal)?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct RawResponse {
            artists: Option<Vec<RawArtist>>,
        }
        #[derive(Deserialize)]
        struct RawArtist {
            id: String,
            name: String,
            disambiguation: Option<String>,
            country: Option<String>,
            score: Option<u32>,
        }

        let raw: RawResponse = resp.json().await.unwrap_or(RawResponse { artists: None });
        Ok(raw
            .artists
            .unwrap_or_default()
            .into_iter()
            .map(|a| MbArtist {
                id: a.id,
                name: a.name,
                disambiguation: a.disambiguation,
                country: a.country,
                score: a.score,
            })
            .collect())
    }

    /// Search for releases (albums) by title and optional artist.
    pub async fn search_release(
        &self,
        title: &str,
        artist: Option<&str>,
    ) -> Result<Vec<MbRelease>> {
        self.wait_rate_limit().await;
        let query = match artist {
            Some(a) if !a.is_empty() => format!("release:\"{}\" AND artist:\"{}\"", title, a),
            _ => format!("release:\"{}\"", title),
        };

        let url = format!("{}/release", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("query", query.as_str()), ("fmt", "json")])
            .send()
            .await
            .map_err(Error::internal)?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct RawResponse {
            releases: Option<Vec<RawRelease>>,
        }
        #[derive(Deserialize)]
        struct RawRelease {
            id: String,
            title: String,
            date: Option<String>,
            country: Option<String>,
            #[serde(rename = "artist-credit")]
            artist_credit: Option<Vec<RawCredit>>,
            #[serde(rename = "track-count")]
            track_count: Option<u32>,
            score: Option<u32>,
        }
        #[derive(Deserialize)]
        struct RawCredit {
            name: String,
        }

        let raw: RawResponse = resp.json().await.unwrap_or(RawResponse { releases: None });
        Ok(raw
            .releases
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let artist_credit =
                    r.artist_credit.and_then(|c| c.into_iter().next().map(|ac| ac.name));
                MbRelease {
                    id: r.id,
                    title: r.title,
                    date: r.date,
                    country: r.country,
                    artist_credit,
                    track_count: r.track_count,
                    score: r.score,
                }
            })
            .collect())
    }

    /// Get Cover Art Archive front cover image URL for a release MBID.
    pub fn get_cover_art_front_url(release_mbid: &str) -> String {
        format!("{}/release/{}/front", CAA_BASE, release_mbid)
    }
}

impl Default for MusicBrainzClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_art_front_url_format() {
        let url =
            MusicBrainzClient::get_cover_art_front_url("76df3287-6cda-33eb-8e9a-044b5e15ffdd");
        assert_eq!(
            url,
            "https://coverartarchive.org/release/76df3287-6cda-33eb-8e9a-044b5e15ffdd/front"
        );
    }
}
