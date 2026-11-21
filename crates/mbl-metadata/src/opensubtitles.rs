//! OpenSubtitles REST API client: search, sign-in and download.
//!
//! Every call needs an API key (`Api-Key`). Downloads also work without an account but are then
//! limited per IP; with a username and password a bearer token from `/login` raises the limit.

use std::time::Duration;

use mbl_core::{Error, Result};
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const OS_BASE: &str = "https://api.opensubtitles.com/api/v1";
/// The service asks clients to name themselves and their version.
const USER_AGENT_VAL: &str = concat!("MBL v", env!("CARGO_PKG_VERSION"));
/// Largest subtitle file accepted.
const MAX_SUBTITLE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleCandidate {
    pub id: String,
    pub language: String,
    pub release_name: String,
    pub download_count: u32,
    pub rating: f64,
    pub hearing_impaired: bool,
    pub file_id: u64,
    pub file_name: String,
}

/// What to search for. Movies by TMDB/IMDb ID; episodes by the show's TMDB ID with season and
/// episode; anything else by name.
#[derive(Debug, Clone, Default)]
pub struct SubtitleQuery<'a> {
    /// Two-letter code (`sv`, `en`, `pt-br`); several separated by commas.
    pub language: &'a str,
    pub imdb_id: Option<&'a str>,
    pub tmdb_id: Option<i64>,
    pub parent_tmdb_id: Option<i64>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub query: Option<&'a str>,
}

/// A downloaded subtitle file.
#[derive(Debug, Clone)]
pub struct DownloadedSubtitle {
    pub bytes: Vec<u8>,
    pub file_name: String,
    /// Downloads left today, when the service says.
    pub remaining: Option<i64>,
}

#[derive(Clone)]
pub struct OpenSubtitlesClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

/// A readable reason from an error response (`message` or `errors`).
fn reason(status: StatusCode, body: &Value) -> String {
    let text = body["message"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| body["errors"].as_array().and_then(|e| e.first()?.as_str().map(str::to_owned)));
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            "OpenSubtitles rejected the API key or sign-in".into()
        }
        StatusCode::NOT_ACCEPTABLE | StatusCode::TOO_MANY_REQUESTS => format!(
            "OpenSubtitles download limit reached{}",
            text.map(|t| format!(": {t}")).unwrap_or_default()
        ),
        _ => format!(
            "OpenSubtitles answered {status}{}",
            text.map(|t| format!(": {t}")).unwrap_or_default()
        ),
    }
}

impl OpenSubtitlesClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(OS_BASE, api_key)
    }

    pub fn with_base_url(base_url: &str, api_key: Option<String>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VAL));
        headers.insert(reqwest::header::ACCEPT, HeaderValue::from_static("application/json"));
        if let Some(ref key) = api_key
            && let Ok(val) = HeaderValue::from_str(key)
        {
            headers.insert("Api-Key", val);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();

        Self { client, base_url: base_url.trim_end_matches('/').to_owned(), api_key }
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value> {
        let resp = req.send().await.map_err(|e| {
            Error::External(format!("Could not reach OpenSubtitles: {}", e.without_url()))
        })?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(Error::External(reason(status, &body)));
        }
        Ok(body)
    }

    /// Search for subtitles, most downloaded first.
    pub async fn search(&self, q: &SubtitleQuery<'_>) -> Result<Vec<SubtitleCandidate>> {
        let mut params: Vec<(&str, String)> = vec![("languages", q.language.to_lowercase())];
        if let Some(imdb) = q.imdb_id {
            params.push(("imdb_id", imdb.trim_start_matches("tt").to_owned()));
        }
        if let Some(id) = q.tmdb_id {
            params.push(("tmdb_id", id.to_string()));
        }
        if let Some(id) = q.parent_tmdb_id {
            params.push(("parent_tmdb_id", id.to_string()));
        }
        if let Some(n) = q.season {
            params.push(("season_number", n.to_string()));
        }
        if let Some(n) = q.episode {
            params.push(("episode_number", n.to_string()));
        }
        if let Some(text) = q.query {
            params.push(("query", text.to_owned()));
        }
        // The service redirects when parameters are not in alphabetical order.
        params.sort_by_key(|p| p.0);
        let body = self
            .send(self.client.get(format!("{}/subtitles", self.base_url)).query(&params))
            .await?;

        let mut candidates = Vec::new();
        for item in body["data"].as_array().into_iter().flatten() {
            let attrs = &item["attributes"];
            for file in attrs["files"].as_array().into_iter().flatten() {
                let Some(file_id) = file["file_id"].as_u64() else { continue };
                candidates.push(SubtitleCandidate {
                    id: item["id"].as_str().unwrap_or_default().to_owned(),
                    language: attrs["language"].as_str().unwrap_or(q.language).to_owned(),
                    release_name: attrs["release"].as_str().unwrap_or_default().to_owned(),
                    download_count: attrs["download_count"]
                        .as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(0),
                    rating: attrs["ratings"].as_f64().unwrap_or(0.0),
                    hearing_impaired: attrs["hearing_impaired"].as_bool().unwrap_or(false),
                    file_id,
                    file_name: file["file_name"].as_str().unwrap_or_default().to_owned(),
                });
            }
        }
        candidates.sort_by_key(|c| std::cmp::Reverse(c.download_count));
        Ok(candidates)
    }

    /// Sign in; returns the bearer token for downloads.
    pub async fn login(&self, username: &str, password: &str) -> Result<String> {
        let body = self
            .send(
                self.client
                    .post(format!("{}/login", self.base_url))
                    .json(&serde_json::json!({ "username": username, "password": password })),
            )
            .await?;
        body["token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::External("OpenSubtitles sign-in returned no token".into()))
    }

    /// Download one file as SRT (UTF-8).
    pub async fn download(&self, file_id: u64, token: Option<&str>) -> Result<DownloadedSubtitle> {
        let mut req = self
            .client
            .post(format!("{}/download", self.base_url))
            .json(&serde_json::json!({ "file_id": file_id, "sub_format": "srt" }));
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        let body = self.send(req).await?;
        let link = body["link"]
            .as_str()
            .ok_or_else(|| Error::External("OpenSubtitles returned no download link".into()))?;
        let resp = self.client.get(link).send().await.map_err(|e| {
            Error::External(format!("Could not download the subtitle: {}", e.without_url()))
        })?;
        if !resp.status().is_success() {
            return Err(Error::External(format!(
                "The subtitle download answered {}",
                resp.status()
            )));
        }
        let bytes = resp.bytes().await.map_err(Error::internal)?;
        if bytes.is_empty() || bytes.len() > MAX_SUBTITLE_BYTES {
            return Err(Error::External("The downloaded subtitle is empty or too large".into()));
        }
        Ok(DownloadedSubtitle {
            bytes: bytes.to_vec(),
            file_name: body["file_name"].as_str().unwrap_or_default().to_owned(),
            remaining: body["remaining"].as_i64(),
        })
    }
}
