//! Radarr (movies), Sonarr (series) and Lidarr (music): settings, connection test, music search
//! and adding titles.
//!
//! Approving a request adds the title to the matching service, which then searches and downloads
//! it. Radarr and Sonarr use API v3, Lidarr API v1, all with an `X-Api-Key` header. Radarr is
//! looked up by TMDB ID; Sonarr keys series by TVDB ID, so it is looked up with `tmdb:<id>`;
//! Lidarr keys artists and albums by MusicBrainz ID (`lidarr:<mbid>`).

use mbl_core::{Actor, Error, Result};
use mbl_db::Db;
use mbl_db::repo::m7::MediaRequestRow;
use mbl_db::repo::settings::SettingsRepo;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ArrKind {
    Radarr,
    Sonarr,
    Lidarr,
}

impl ArrKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "radarr" => Ok(Self::Radarr),
            "sonarr" => Ok(Self::Sonarr),
            "lidarr" => Ok(Self::Lidarr),
            _ => Err(Error::Validation("Use radarr, sonarr or lidarr".into())),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Radarr => "radarr",
            Self::Sonarr => "sonarr",
            Self::Lidarr => "lidarr",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Radarr => "Radarr",
            Self::Sonarr => "Sonarr",
            Self::Lidarr => "Lidarr",
        }
    }

    fn api(self) -> &'static str {
        match self {
            Self::Lidarr => "/api/v1",
            Self::Radarr | Self::Sonarr => "/api/v3",
        }
    }

    /// The service that handles a request of this kind.
    pub fn for_request(kind: &str) -> Option<Self> {
        match kind {
            "movie" => Some(Self::Radarr),
            "show" | "series" => Some(Self::Sonarr),
            "artist" | "album" => Some(Self::Lidarr),
            _ => None,
        }
    }
}

/// Stored settings for one service. The API key is never returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArrSettings {
    pub kind: ArrKind,
    pub enabled: bool,
    pub url: String,
    pub has_api_key: bool,
    pub root_folder: String,
    pub quality_profile_id: Option<i64>,
    /// Lidarr only.
    pub metadata_profile_id: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateArrSettings {
    pub enabled: bool,
    pub url: String,
    /// New key; omit or leave empty to keep the stored one.
    pub api_key: Option<String>,
    pub root_folder: String,
    pub quality_profile_id: Option<i64>,
    /// Lidarr only: which releases (albums, singles, EPs…) an added artist gets.
    #[serde(default)]
    pub metadata_profile_id: Option<i64>,
}

/// Connection test input. Without `apiKey` the stored key is used.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestArrConnection {
    pub url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArrOption {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArrConnectionReport {
    pub version: Option<String>,
    pub root_folders: Vec<String>,
    pub quality_profiles: Vec<ArrOption>,
    /// Lidarr only; empty for Radarr and Sonarr.
    pub metadata_profiles: Vec<ArrOption>,
}

/// An artist or album found through Lidarr, ready to be requested.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MusicSearchHit {
    /// "artist" or "album".
    pub kind: String,
    pub title: String,
    /// Artist name of an album.
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub musicbrainz_id: String,
    pub image_url: Option<String>,
    /// Already in Lidarr.
    pub in_lidarr: bool,
}

#[derive(Debug, Clone)]
pub struct ArrService {
    db: Db,
    http: reqwest::Client,
}

fn setting_key(kind: ArrKind, field: &str) -> String {
    format!("{}_{field}", kind.key())
}

fn clean_url(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(url).map_err(|_| {
        Error::Validation("Enter the full address, e.g. http://192.168.1.10:7878".into())
    })?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(Error::Validation("The address must start with http:// or https://".into()));
    }
    Ok(url.to_owned())
}

fn options(list: &Value) -> Vec<ArrOption> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(ArrOption { id: p["id"].as_i64()?, name: p["name"].as_str()?.to_owned() })
        })
        .collect()
}

/// First poster/cover URL of a lookup result.
fn image(item: &Value) -> Option<String> {
    let images = item["images"].as_array()?;
    let pick = |kind: &str| {
        images
            .iter()
            .find(|i| i["coverType"] == kind)
            .and_then(|i| i["remoteUrl"].as_str().or_else(|| i["url"].as_str()).map(str::to_owned))
    };
    pick("poster").or_else(|| pick("cover")).or_else(|| pick("fanart"))
}

/// A link that opens a download service in the browser. Only for admins: it shows the service's
/// address on the home network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArrLink {
    pub service: ArrKind,
    pub url: String,
    /// True when the link opens the title itself, false for the service's start page.
    pub direct: bool,
}

/// Page of a title in the service's web interface, from the title as the service returns it.
fn title_page(kind: ArrKind, what: &str, item: &Value) -> Option<String> {
    let (page, field) = match (kind, what) {
        (ArrKind::Radarr, _) => ("movie", "titleSlug"),
        (ArrKind::Sonarr, _) => ("series", "titleSlug"),
        (ArrKind::Lidarr, "album") => ("album", "foreignAlbumId"),
        (ArrKind::Lidarr, _) => ("artist", "foreignArtistId"),
    };
    // Slugs and MusicBrainz IDs are letters, digits and hyphens; anything else stays out of the path.
    let value = item[field].as_str()?;
    let safe = !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    safe.then(|| format!("/{page}/{value}"))
}

/// Connection details for an enabled-or-not service that has an address and a key.
struct Conn {
    url: String,
    key: String,
}

impl ArrService {
    pub fn new(db: Db) -> Self {
        let http =
            reqwest::Client::builder().timeout(Duration::from_secs(20)).build().unwrap_or_default();
        Self { db, http }
    }

    fn settings(&self) -> SettingsRepo {
        SettingsRepo::new(self.db.clone())
    }

    async fn stored(&self, kind: ArrKind, field: &str) -> Result<Option<String>> {
        Ok(self.settings().get(&setting_key(kind, field)).await?.filter(|v| !v.is_empty()))
    }

    async fn stored_id(&self, kind: ArrKind, field: &str) -> Result<Option<i64>> {
        Ok(self.stored(kind, field).await?.and_then(|v| v.parse().ok()))
    }

    async fn conn(&self, kind: ArrKind) -> Result<Conn> {
        let url = self
            .stored(kind, "url")
            .await?
            .ok_or_else(|| Error::Validation(format!("{} has no address", kind.name())))?;
        let key = self
            .stored(kind, "api_key")
            .await?
            .ok_or_else(|| Error::Validation(format!("{} has no API key", kind.name())))?;
        Ok(Conn { url: clean_url(&url)?, key })
    }

    pub async fn get_settings(&self, actor: &Actor, kind: ArrKind) -> Result<ArrSettings> {
        actor.require_admin()?;
        Ok(ArrSettings {
            kind,
            enabled: self.stored(kind, "enabled").await?.as_deref() == Some("true"),
            url: self.stored(kind, "url").await?.unwrap_or_default(),
            has_api_key: self.stored(kind, "api_key").await?.is_some(),
            root_folder: self.stored(kind, "root_folder").await?.unwrap_or_default(),
            quality_profile_id: self.stored_id(kind, "quality_profile_id").await?,
            metadata_profile_id: self.stored_id(kind, "metadata_profile_id").await?,
        })
    }

    pub async fn update_settings(
        &self,
        actor: &Actor,
        kind: ArrKind,
        update: UpdateArrSettings,
    ) -> Result<ArrSettings> {
        actor.require_admin()?;
        let url =
            if update.url.trim().is_empty() { String::new() } else { clean_url(&update.url)? };
        let new_key = update.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
        let has_key = new_key.is_some() || self.stored(kind, "api_key").await?.is_some();
        if update.enabled {
            if url.is_empty() || !has_key {
                return Err(Error::Validation(format!(
                    "{} needs an address and an API key before it can be turned on",
                    kind.name()
                )));
            }
            let missing_metadata = kind == ArrKind::Lidarr && update.metadata_profile_id.is_none();
            if update.root_folder.trim().is_empty()
                || update.quality_profile_id.is_none()
                || missing_metadata
            {
                return Err(Error::Validation(
                    "Choose a root folder and the profiles (use Test connection to list them)"
                        .into(),
                ));
            }
        }
        let id = |v: Option<i64>| v.map(|p| p.to_string()).unwrap_or_default();
        let mut entries = vec![
            (setting_key(kind, "enabled"), if update.enabled { "true" } else { "false" }.into()),
            (setting_key(kind, "url"), url),
            (setting_key(kind, "root_folder"), update.root_folder.trim().to_owned()),
            (setting_key(kind, "quality_profile_id"), id(update.quality_profile_id)),
        ];
        if kind == ArrKind::Lidarr {
            entries
                .push((setting_key(kind, "metadata_profile_id"), id(update.metadata_profile_id)));
        }
        if let Some(key) = new_key {
            entries.push((setting_key(kind, "api_key"), key));
        }
        let borrowed: Vec<(&str, &str)> =
            entries.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.settings().set_many(&borrowed).await?;
        tracing::info!(
            service = kind.name(),
            enabled = update.enabled,
            "download service settings saved"
        );
        self.get_settings(actor, kind).await
    }

    async fn call(&self, url: &str, key: &str, path: &str) -> Result<Value> {
        let resp =
            self.http.get(format!("{url}{path}")).header("X-Api-Key", key).send().await.map_err(
                |e| Error::External(format!("Could not reach {url}: {}", e.without_url())),
            )?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::External("The API key was rejected".into()));
        }
        if !status.is_success() {
            let path = path.split('?').next().unwrap_or(path);
            return Err(Error::External(format!("{path} answered {status}")));
        }
        resp.json().await.map_err(|_| Error::External(format!("{path} did not return JSON")))
    }

    /// Check address and key, and list the root folders and profiles to choose from.
    pub async fn test_connection(
        &self,
        actor: &Actor,
        kind: ArrKind,
        input: TestArrConnection,
    ) -> Result<ArrConnectionReport> {
        actor.require_admin()?;
        let url = clean_url(&input.url)?;
        let key = match input.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty()) {
            Some(k) => k,
            None => self
                .stored(kind, "api_key")
                .await?
                .ok_or_else(|| Error::Validation("Enter the API key".into()))?,
        };
        let api = kind.api();
        let status = self.call(&url, &key, &format!("{api}/system/status")).await?;
        let app = status["appName"].as_str().unwrap_or_default();
        if !app.is_empty() && !app.eq_ignore_ascii_case(kind.name()) {
            return Err(Error::Validation(format!("That address is {app}, not {}", kind.name())));
        }
        let folders = self.call(&url, &key, &format!("{api}/rootfolder")).await?;
        let profiles = self.call(&url, &key, &format!("{api}/qualityprofile")).await?;
        let metadata = if kind == ArrKind::Lidarr {
            self.call(&url, &key, &format!("{api}/metadataprofile")).await?
        } else {
            Value::Null
        };
        Ok(ArrConnectionReport {
            version: status["version"].as_str().map(str::to_owned),
            root_folders: folders
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f["path"].as_str().map(str::to_owned))
                .collect(),
            quality_profiles: options(&profiles),
            metadata_profiles: options(&metadata),
        })
    }

    /// Search artists or albums through Lidarr (it queries MusicBrainz), so music can be
    /// requested without knowing the MusicBrainz ID.
    pub async fn search_music(
        &self,
        actor: &Actor,
        kind: &str,
        query: &str,
    ) -> Result<Vec<MusicSearchHit>> {
        let _ = actor;
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::Validation("Enter a name to search for".into()));
        }
        let Conn { url, key } = self.conn(ArrKind::Lidarr).await.map_err(|_| {
            Error::Unavailable(
                "Music search needs Lidarr; ask an administrator to connect it".into(),
            )
        })?;
        let path = match kind {
            "artist" => "/api/v1/artist/lookup",
            "album" => "/api/v1/album/lookup",
            _ => return Err(Error::Validation("Search for an artist or an album".into())),
        };
        let mut lookup = reqwest::Url::parse("http://x").map_err(Error::internal)?;
        lookup.set_path(path);
        lookup.query_pairs_mut().append_pair("term", query);
        let path = format!("{}?{}", lookup.path(), lookup.query().unwrap_or_default());
        let hits = self.call(&url, &key, &path).await?;
        Ok(hits
            .as_array()
            .into_iter()
            .flatten()
            .take(20)
            .filter_map(|h| {
                let (title, mbid, artist) = if kind == "artist" {
                    (h["artistName"].as_str()?, h["foreignArtistId"].as_str()?, None)
                } else {
                    let artist = h["artist"]["artistName"].as_str().map(str::to_owned);
                    (h["title"].as_str()?, h["foreignAlbumId"].as_str()?, artist)
                };
                Some(MusicSearchHit {
                    kind: kind.to_owned(),
                    title: title.to_owned(),
                    artist,
                    year: h["releaseDate"]
                        .as_str()
                        .and_then(|d| d.get(..4))
                        .and_then(|y| y.parse().ok()),
                    musicbrainz_id: mbid.to_owned(),
                    image_url: image(h),
                    in_lidarr: h["id"].as_i64().is_some_and(|id| id > 0),
                })
            })
            .collect())
    }

    /// Start pages of the services that have an address and a key, for the "Open in …" links.
    pub async fn links(&self, actor: &Actor) -> Result<Vec<ArrLink>> {
        actor.require_admin()?;
        let mut links = Vec::new();
        for kind in [ArrKind::Radarr, ArrKind::Sonarr, ArrKind::Lidarr] {
            if let Ok(Conn { url, .. }) = self.conn(kind).await {
                links.push(ArrLink { service: kind, url, direct: false });
            }
        }
        Ok(links)
    }

    /// Link to a title in the service that handles `what` (movie, show, artist or album):
    /// straight to the title when the service has it, otherwise its start page. `None` when that
    /// service has no address or key. `arr_id` is the service's own ID; without it a movie or
    /// show is looked up by TMDB ID among the service's titles.
    pub async fn title_link(
        &self,
        actor: &Actor,
        what: &str,
        arr_id: Option<i64>,
        tmdb_id: Option<i64>,
    ) -> Result<Option<ArrLink>> {
        actor.require_admin()?;
        let Some(kind) = ArrKind::for_request(what) else { return Ok(None) };
        let Ok(Conn { url, key }) = self.conn(kind).await else { return Ok(None) };
        let found = match (arr_id, tmdb_id) {
            (Some(id), _) => {
                let path = match (kind, what) {
                    (ArrKind::Lidarr, "album") => format!("/api/v1/album/{id}"),
                    (ArrKind::Lidarr, _) => format!("/api/v1/artist/{id}"),
                    (ArrKind::Radarr, _) => format!("/api/v3/movie/{id}"),
                    (ArrKind::Sonarr, _) => format!("/api/v3/series/{id}"),
                };
                self.call(&url, &key, &path).await.ok()
            }
            (None, Some(tmdb)) if kind == ArrKind::Radarr => self
                .call(&url, &key, &format!("/api/v3/movie?tmdbId={tmdb}"))
                .await
                .ok()
                .and_then(|list| list.as_array()?.first().cloned()),
            (None, Some(tmdb)) if kind == ArrKind::Sonarr => {
                self.call(&url, &key, "/api/v3/series").await.ok().and_then(|list| {
                    list.as_array()?.iter().find(|s| s["tmdbId"].as_i64() == Some(tmdb)).cloned()
                })
            }
            _ => None,
        };
        let page = found.as_ref().and_then(|item| title_page(kind, what, item));
        Ok(Some(ArrLink {
            service: kind,
            direct: page.is_some(),
            url: format!("{url}{}", page.unwrap_or_default()),
        }))
    }

    /// Add a request's title to the service that handles its kind. Returns the service and its
    /// ID, or `None` when that service is not turned on (the request is then only approved).
    pub async fn add(&self, req: &MediaRequestRow) -> Result<Option<(ArrKind, i64)>> {
        let Some(kind) = ArrKind::for_request(&req.kind) else { return Ok(None) };
        if self.stored(kind, "enabled").await?.as_deref() != Some("true") {
            return Ok(None);
        }
        let Conn { url, key } = self.conn(kind).await?;
        let root = self.stored(kind, "root_folder").await?.unwrap_or_default();
        let profile = self
            .stored_id(kind, "quality_profile_id")
            .await?
            .ok_or_else(|| Error::Validation(format!("{} has no quality profile", kind.name())))?;
        let title = &req.title;
        let none = |what: &str| {
            Error::External(format!("{} found no {what} for \"{title}\"", kind.name()))
        };
        let first = |found: Value, what: &str| {
            found.as_array().and_then(|a| a.first().cloned()).ok_or_else(|| none(what))
        };
        let mbid = req.musicbrainz_id.as_deref().unwrap_or_default();

        let mut item = match req.kind.as_str() {
            "movie" => {
                let path = format!("/api/v3/movie/lookup/tmdb?tmdbId={}", req.tmdb_id);
                self.call(&url, &key, &path).await?
            }
            "artist" => {
                let path = format!("/api/v1/artist/lookup?term=lidarr:{mbid}");
                first(self.call(&url, &key, &path).await?, "artist")?
            }
            "album" => {
                let path = format!("/api/v1/album/lookup?term=lidarr:{mbid}");
                first(self.call(&url, &key, &path).await?, "album")?
            }
            _ => {
                let path = format!("/api/v3/series/lookup?term=tmdb:{}", req.tmdb_id);
                first(self.call(&url, &key, &path).await?, "series")?
            }
        };
        if !item.is_object() {
            return Err(none("match"));
        }
        // Already in the service: nothing to add.
        if let Some(id) = item["id"].as_i64().filter(|id| *id > 0) {
            tracing::info!(service = kind.name(), id, title, "title already in download service");
            return Ok(Some((kind, id)));
        }

        let path = match req.kind.as_str() {
            "movie" => {
                item["qualityProfileId"] = json!(profile);
                item["rootFolderPath"] = json!(root);
                item["monitored"] = json!(true);
                item["minimumAvailability"] = json!("released");
                item["addOptions"] = json!({ "searchForMovie": true });
                "/api/v3/movie"
            }
            "artist" | "album" => {
                let metadata = self
                    .stored_id(kind, "metadata_profile_id")
                    .await?
                    .ok_or_else(|| Error::Validation("Lidarr has no metadata profile".into()))?;
                let artist_settings = json!({
                    "qualityProfileId": profile,
                    "metadataProfileId": metadata,
                    "rootFolderPath": root,
                    "monitored": true,
                });
                if req.kind == "artist" {
                    merge(&mut item, &artist_settings);
                    item["addOptions"] =
                        json!({ "monitor": "all", "searchForMissingAlbums": true });
                    "/api/v1/artist"
                } else {
                    // An album is added together with its artist; only this album is monitored.
                    if item["artist"].is_object() {
                        merge(&mut item["artist"], &artist_settings);
                        item["artist"]["addOptions"] =
                            json!({ "monitor": "none", "searchForMissingAlbums": false });
                    }
                    item["monitored"] = json!(true);
                    item["addOptions"] = json!({ "searchForNewAlbum": true });
                    "/api/v1/album"
                }
            }
            _ => {
                item["qualityProfileId"] = json!(profile);
                item["rootFolderPath"] = json!(root);
                item["monitored"] = json!(true);
                item["seasonFolder"] = json!(true);
                item["addOptions"] = json!({ "monitor": "all", "searchForMissingEpisodes": true });
                // Sonarr v3 also requires a language profile; v4 has none.
                if let Ok(langs) = self.call(&url, &key, "/api/v3/languageprofile").await
                    && let Some(id) =
                        langs.as_array().and_then(|a| a.first()).and_then(|l| l["id"].as_i64())
                {
                    item["languageProfileId"] = json!(id);
                }
                "/api/v3/series"
            }
        };
        let resp = self
            .http
            .post(format!("{url}{path}"))
            .header("X-Api-Key", &key)
            .json(&item)
            .send()
            .await
            .map_err(|e| Error::External(format!("Could not reach {url}: {}", e.without_url())))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::External(format!("{}: the API key was rejected", kind.name())));
        }
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            // Validation errors come back as [{ "errorMessage": … }].
            let reason = body
                .as_array()
                .and_then(|a| a.first())
                .and_then(|e| e["errorMessage"].as_str())
                .map_or_else(|| status.to_string(), str::to_owned);
            return Err(Error::External(format!(
                "{} did not add \"{title}\": {reason}",
                kind.name()
            )));
        }
        let id = body["id"].as_i64().ok_or_else(|| {
            Error::External(format!("{} added \"{title}\" but returned no ID", kind.name()))
        })?;
        tracing::info!(service = kind.name(), id, title, "title added to download service");
        Ok(Some((kind, id)))
    }
}

/// Copy the fields of `extra` into the object `target`.
fn merge(target: &mut Value, extra: &Value) {
    if let (Some(t), Some(e)) = (target.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            t.insert(k.clone(), v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_pages_follow_each_service() {
        let movie = json!({ "titleSlug": "the-matrix-603" });
        assert_eq!(
            title_page(ArrKind::Radarr, "movie", &movie).as_deref(),
            Some("/movie/the-matrix-603")
        );
        let show = json!({ "titleSlug": "dark-matter" });
        assert_eq!(
            title_page(ArrKind::Sonarr, "show", &show).as_deref(),
            Some("/series/dark-matter")
        );
        let mbid = "a74b1b7f-71a5-4011-9441-d0b5e4122711";
        let artist = json!({ "foreignArtistId": mbid });
        assert_eq!(title_page(ArrKind::Lidarr, "artist", &artist), Some(format!("/artist/{mbid}")));
        let album = json!({ "foreignAlbumId": mbid });
        assert_eq!(title_page(ArrKind::Lidarr, "album", &album), Some(format!("/album/{mbid}")));
        // Nothing usable, or something that would change the path, gives the start page.
        assert_eq!(title_page(ArrKind::Radarr, "movie", &json!({})), None);
        assert_eq!(title_page(ArrKind::Radarr, "movie", &json!({ "titleSlug": "../x" })), None);
        assert_eq!(title_page(ArrKind::Sonarr, "show", &json!({ "titleSlug": "" })), None);
    }
}
