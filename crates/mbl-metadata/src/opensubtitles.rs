//! OpenSubtitles REST API client (M7.8).

use std::time::Duration;

use mbl_core::{Error, Result};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};

const OS_BASE: &str = "https://api.opensubtitles.com/api/v1";
const USER_AGENT_VAL: &str = "MBL v0.6.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleCandidate {
    pub id: String,
    pub language: String,
    pub release_name: String,
    pub download_count: u32,
    pub rating: f64,
    pub file_id: u64,
    pub file_name: String,
}

#[derive(Clone)]
pub struct OpenSubtitlesClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
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
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        Self { client, base_url: base_url.trim_end_matches('/').to_owned(), api_key }
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Search subtitles by IMDB ID, TMDB ID, or query.
    pub async fn search_subtitles(
        &self,
        imdb_id: Option<&str>,
        tmdb_id: Option<i64>,
        language: &str,
    ) -> Result<Vec<SubtitleCandidate>> {
        let url = format!("{}/subtitles", self.base_url);
        let mut req = self.client.get(&url).query(&[("languages", language)]);

        if let Some(imdb) = imdb_id {
            let clean = imdb.trim_start_matches("tt");
            req = req.query(&[("imdb_id", clean)]);
        }
        if let Some(tmdb) = tmdb_id {
            req = req.query(&[("tmdb_id", &tmdb.to_string())]);
        }

        let resp = req.send().await.map_err(Error::internal)?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct RawResponse {
            data: Option<Vec<RawData>>,
        }
        #[derive(Deserialize)]
        struct RawData {
            id: String,
            attributes: Option<RawAttrs>,
        }
        #[derive(Deserialize)]
        struct RawAttrs {
            language: Option<String>,
            release: Option<String>,
            download_count: Option<u32>,
            ratings: Option<f64>,
            files: Option<Vec<RawFile>>,
        }
        #[derive(Deserialize)]
        struct RawFile {
            file_id: u64,
            file_name: String,
        }

        let raw: RawResponse = resp.json().await.unwrap_or(RawResponse { data: None });
        let mut candidates = Vec::new();

        for item in raw.data.unwrap_or_default() {
            if let Some(attrs) = item.attributes
                && let Some(files) = attrs.files
            {
                for f in files {
                    candidates.push(SubtitleCandidate {
                        id: item.id.clone(),
                        language: attrs.language.clone().unwrap_or_else(|| language.to_owned()),
                        release_name: attrs.release.clone().unwrap_or_default(),
                        download_count: attrs.download_count.unwrap_or(0),
                        rating: attrs.ratings.unwrap_or(0.0),
                        file_id: f.file_id,
                        file_name: f.file_name,
                    });
                }
            }
        }

        Ok(candidates)
    }

    /// Request download URL and fetch subtitle content.
    pub async fn download_subtitle(&self, file_id: u64) -> Result<Vec<u8>> {
        let url = format!("{}/download", self.base_url);
        #[derive(Serialize)]
        struct DownloadReq {
            file_id: u64,
        }
        #[derive(Deserialize)]
        struct DownloadResp {
            link: Option<String>,
        }

        let resp = self
            .client
            .post(&url)
            .json(&DownloadReq { file_id })
            .send()
            .await
            .map_err(Error::internal)?;

        if !resp.status().is_success() {
            return Err(Error::External("Failed to request subtitle download link".into()));
        }

        let body: DownloadResp = resp.json().await.map_err(Error::internal)?;
        let link = body
            .link
            .ok_or_else(|| Error::External("Missing subtitle download link in response".into()))?;

        let dl = self.client.get(&link).send().await.map_err(Error::internal)?;
        let bytes = dl.bytes().await.map_err(Error::internal)?;
        Ok(bytes.to_vec())
    }
}
