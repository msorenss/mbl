//! Find and download subtitles from OpenSubtitles.
//!
//! A downloaded subtitle is saved next to the video as `<name>.<language>.srt` (the same naming
//! the scanner reads, so it survives a rescan) and added to the file's tracks right away.

use std::path::{Path, PathBuf};

use mbl_core::{Actor, Error, ItemId, MediaKind, PartId, Result};
use mbl_db::Db;
use mbl_db::repo::settings::SettingsRepo;
use mbl_metadata::{OpenSubtitlesClient, SubtitleQuery};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{ItemService, StreamService};

const KEY_API: &str = "opensubtitles_api_key";
const KEY_USER: &str = "opensubtitles_username";
const KEY_PASSWORD: &str = "opensubtitles_password";

/// OpenSubtitles settings as shown to an admin; the key and password are never returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenSubtitlesSettings {
    pub has_api_key: bool,
    pub username: String,
    pub has_password: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOpenSubtitlesSettings {
    /// New API key; leave out to keep the stored one.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub username: String,
    /// New password; leave out to keep the stored one.
    #[serde(default)]
    pub password: Option<String>,
    /// Remove the stored key, username and password.
    #[serde(default)]
    pub clear: bool,
}

/// One subtitle file found.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSearchHit {
    pub file_id: u64,
    pub language: String,
    pub release: String,
    pub file_name: String,
    pub downloads: u32,
    pub rating: f64,
    pub hearing_impaired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSubtitleRequest {
    pub file_id: u64,
    /// Language of the file (`sv`, `en`…), used in the file name.
    pub language: String,
    #[serde(default)]
    pub hearing_impaired: bool,
}

/// A subtitle saved next to the video and added as a track.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedSubtitleTrack {
    pub part_id: PartId,
    /// Track index to choose in the player.
    pub stream_index: u32,
    pub language: String,
    pub file_name: String,
    /// Downloads left today, when OpenSubtitles says.
    pub remaining: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SubtitleSearchService {
    db: Db,
    items: ItemService,
    stream: StreamService,
    base_url: Option<String>,
}

/// `sv`, `en`, `pt-br`: letters and one optional region.
fn clean_language(raw: &str) -> Result<String> {
    let lang = raw.trim().to_ascii_lowercase();
    let ok = match lang.split_once('-') {
        Some((l, r)) => {
            l.len() == 2
                && l.chars().all(|c| c.is_ascii_lowercase())
                && (2..=3).contains(&r.len())
                && r.chars().all(|c| c.is_ascii_lowercase())
        }
        None => lang.len() == 2 && lang.chars().all(|c| c.is_ascii_lowercase()),
    };
    if !ok {
        return Err(Error::Validation("Choose a language such as sv or en".into()));
    }
    Ok(lang)
}

/// `<dir>/<stem>.<lang>[.sdh][.<n>].srt` that does not exist yet.
fn free_sidecar(video: &Path, language: &str, sdh: bool) -> Result<PathBuf> {
    let dir = video.parent().ok_or(Error::NotFound)?;
    let stem = video.file_stem().and_then(|s| s.to_str()).ok_or(Error::NotFound)?;
    let tags = if sdh { format!("{language}.sdh") } else { language.to_owned() };
    for n in 0..100 {
        let name =
            if n == 0 { format!("{stem}.{tags}.srt") } else { format!("{stem}.{tags}.{n}.srt") };
        let path = dir.join(name);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(Error::Conflict("there are already many subtitles in this language".into()))
}

/// Subtitle text as UTF-8 (read as Latin-1 when the file is not UTF-8, as older files often are).
fn subtitle_text(bytes: &[u8]) -> Result<String> {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.trim_start_matches('\u{feff}').to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    };
    if !text.contains("-->") {
        return Err(Error::External("The downloaded file is not a subtitle".into()));
    }
    Ok(text)
}

impl SubtitleSearchService {
    pub fn new(db: Db, stream: StreamService) -> Self {
        Self { items: ItemService::new(db.clone()), db, stream, base_url: None }
    }

    /// Use another OpenSubtitles address (tests).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    fn settings(&self) -> SettingsRepo {
        SettingsRepo::new(self.db.clone())
    }

    async fn stored(&self, key: &str) -> Result<Option<String>> {
        Ok(self.settings().get(key).await?.filter(|v| !v.is_empty()))
    }

    pub async fn get_settings(&self, actor: &Actor) -> Result<OpenSubtitlesSettings> {
        actor.require_admin()?;
        Ok(OpenSubtitlesSettings {
            has_api_key: self.stored(KEY_API).await?.is_some(),
            username: self.stored(KEY_USER).await?.unwrap_or_default(),
            has_password: self.stored(KEY_PASSWORD).await?.is_some(),
        })
    }

    pub async fn update_settings(
        &self,
        actor: &Actor,
        update: UpdateOpenSubtitlesSettings,
    ) -> Result<OpenSubtitlesSettings> {
        actor.require_admin()?;
        let repo = self.settings();
        if update.clear {
            for key in [KEY_API, KEY_USER, KEY_PASSWORD] {
                repo.delete(key).await?;
            }
            return self.get_settings(actor).await;
        }
        if let Some(key) = update.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty()) {
            repo.set(KEY_API, &key).await?;
        }
        repo.set(KEY_USER, update.username.trim()).await?;
        if let Some(password) = update.password.filter(|p| !p.is_empty()) {
            repo.set(KEY_PASSWORD, &password).await?;
        }
        self.get_settings(actor).await
    }

    /// Whether searching is possible (an API key is stored).
    pub async fn available(&self) -> Result<bool> {
        Ok(self.stored(KEY_API).await?.is_some())
    }

    async fn client(&self) -> Result<OpenSubtitlesClient> {
        let key = self.stored(KEY_API).await?.ok_or_else(|| {
            Error::Validation(
                "Subtitle search is not set up: an admin adds an OpenSubtitles API key under \
                 Server settings"
                    .into(),
            )
        })?;
        // Another address (a test server) can be set in the settings table; there is no API for it.
        let base = match &self.base_url {
            Some(url) => Some(url.clone()),
            None => self.stored("opensubtitles_base_url").await?,
        };
        Ok(match base {
            Some(url) => OpenSubtitlesClient::with_base_url(&url, Some(key)),
            None => OpenSubtitlesClient::new(Some(key)),
        })
    }

    /// Check the key (and sign-in, when a username is stored).
    pub async fn test(&self, actor: &Actor) -> Result<()> {
        actor.require_admin()?;
        let client = self.client().await?;
        if let (Some(user), Some(password)) =
            (self.stored(KEY_USER).await?, self.stored(KEY_PASSWORD).await?)
        {
            client.login(&user, &password).await?;
        } else {
            client
                .search(&SubtitleQuery {
                    language: "en",
                    imdb_id: Some("0133093"),
                    ..Default::default()
                })
                .await?;
        }
        Ok(())
    }

    /// Who may fetch subtitles: anyone who can see the title, except managed profiles.
    async fn video(&self, actor: &Actor, item: ItemId) -> Result<mbl_db::repo::items::ItemDetail> {
        if actor.is_managed {
            return Err(Error::Forbidden);
        }
        let detail = self.items.get(actor, item).await?;
        if !matches!(detail.kind, MediaKind::Movie | MediaKind::Episode) {
            return Err(Error::Validation("Subtitles can be found for movies and episodes".into()));
        }
        Ok(detail)
    }

    pub async fn search(
        &self,
        actor: &Actor,
        item: ItemId,
        language: &str,
    ) -> Result<Vec<SubtitleSearchHit>> {
        let language = clean_language(language)?;
        let detail = self.video(actor, item).await?;
        let client = self.client().await?;
        let name = detail.title.clone();
        let mut query = SubtitleQuery { language: &language, ..Default::default() };
        let mut show_tmdb = None;
        if detail.kind == MediaKind::Episode {
            // Episode → season → show.
            let season = match detail.parent_id {
                Some(id) => self.items.get(actor, id).await.ok(),
                None => None,
            };
            let show = match season.as_ref().and_then(|s| s.parent_id) {
                Some(id) => self.items.get(actor, id).await.ok(),
                None => None,
            };
            show_tmdb = show.as_ref().and_then(|s| s.tmdb_id);
            query.parent_tmdb_id = show_tmdb;
            query.season = detail.parent_idx.or(season.as_ref().and_then(|s| s.idx));
            query.episode = detail.idx;
        } else {
            query.tmdb_id = detail.tmdb_id;
            query.imdb_id = detail.imdb_id.as_deref();
        }
        if query.tmdb_id.is_none() && query.imdb_id.is_none() && show_tmdb.is_none() {
            query.query = Some(&name);
        }
        let found = client.search(&query).await?;
        Ok(found
            .into_iter()
            .take(50)
            .map(|c| SubtitleSearchHit {
                file_id: c.file_id,
                language: c.language,
                release: c.release_name,
                file_name: c.file_name,
                downloads: c.download_count,
                rating: c.rating,
                hearing_impaired: c.hearing_impaired,
            })
            .collect())
    }

    /// Download a subtitle next to the title's video and add it as a track.
    pub async fn download(
        &self,
        actor: &Actor,
        item: ItemId,
        req: DownloadSubtitleRequest,
    ) -> Result<DownloadedSubtitleTrack> {
        let language = clean_language(&req.language)?;
        let detail = self.video(actor, item).await?;
        let parts = mbl_db::repo::media::parts_for_item(&self.db, detail.id).await?;
        let part = parts
            .iter()
            .find(|p| p.missing_since.is_none())
            .ok_or_else(|| Error::Validation("This title has no video file".into()))?;
        let resolved = self.stream.resolve_part(actor, part.id).await?;
        if resolved.path.to_str().is_some_and(|p| p.starts_with(mbl_core::NODE_URI_SCHEME)) {
            return Err(Error::Validation(
                "Subtitles cannot be saved next to files on a storage node yet".into(),
            ));
        }
        let target = free_sidecar(&resolved.path, &language, req.hearing_impaired)?;

        let client = self.client().await?;
        let token = match (self.stored(KEY_USER).await?, self.stored(KEY_PASSWORD).await?) {
            (Some(user), Some(password)) => Some(client.login(&user, &password).await?),
            _ => None,
        };
        let file = client.download(req.file_id, token.as_deref()).await?;
        let text = subtitle_text(&file.bytes)?;
        tokio::fs::write(&target, text.as_bytes()).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Error::Validation(format!(
                    "MBL may not write in the folder {}; give the MBL service write access there",
                    target.parent().map(|p| p.display().to_string()).unwrap_or_default()
                ))
            } else {
                Error::internal(e)
            }
        })?;

        let target_str = target.to_str().ok_or(Error::NotFound)?.to_owned();
        let stream_index = mbl_db::repo::media::add_external_subtitle(
            &self.db,
            part.id,
            &language,
            req.hearing_impaired,
            &target_str,
        )
        .await?;
        tracing::info!(part = part.id.0, stream_index, "subtitle downloaded");
        Ok(DownloadedSubtitleTrack {
            part_id: part.id,
            stream_index,
            language,
            file_name: target.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned(),
            remaining: file.remaining,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_and_file_names() {
        assert_eq!(clean_language(" SV ").unwrap(), "sv");
        assert_eq!(clean_language("pt-BR").unwrap(), "pt-br");
        assert!(clean_language("swedish").is_err());
        assert!(clean_language("../x").is_err());

        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("Heat (1995).mkv");
        let first = free_sidecar(&video, "sv", false).unwrap();
        assert_eq!(first.file_name().unwrap(), "Heat (1995).sv.srt");
        std::fs::write(&first, "x").unwrap();
        assert_eq!(
            free_sidecar(&video, "sv", false).unwrap().file_name().unwrap(),
            "Heat (1995).sv.1.srt"
        );
        assert_eq!(
            free_sidecar(&video, "sv", true).unwrap().file_name().unwrap(),
            "Heat (1995).sv.sdh.srt"
        );
    }

    #[test]
    fn only_subtitle_text_is_saved() {
        let srt = "\u{feff}1\n00:00:01,000 --> 00:00:02,000\nHej\n";
        assert!(subtitle_text(srt.as_bytes()).unwrap().starts_with('1'));
        assert_eq!(
            subtitle_text(b"1\n00:00:01,000 --> 00:00:02,000\nCaf\xe9\n").unwrap().lines().nth(2),
            Some("Café")
        );
        assert!(subtitle_text(b"<html>").is_err());
    }
}
