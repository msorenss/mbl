//! Manual metadata editing: change fields, lock them against automatic matching, match an
//! item to another TMDB entry and choose poster or backdrop artwork.

use std::time::Duration;

use mbl_core::events::{Event, EventBus};
use mbl_core::{Actor, Error, ItemId, MediaKind, Result, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::metadata::{self, EditableRow};
use mbl_db::repo::settings::SettingsRepo;
use mbl_jobs::{JobQueue, Priority, kinds};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

/// Fields that can be locked. A locked field keeps its value when metadata is matched again.
pub const LOCKABLE: &[&str] = &[
    "title",
    "original_title",
    "summary",
    "tagline",
    "year",
    "content_rating",
    "studio",
    "genres",
    "poster",
    "backdrop",
];

const TMDB_IMAGES: &str = "https://image.tmdb.org/t/p/";

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemMetadata {
    pub item_id: ItemId,
    pub kind: MediaKind,
    pub title: String,
    pub sort_title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    pub content_rating: Option<String>,
    pub studio: Option<String>,
    pub genres: Vec<String>,
    pub tmdb_id: Option<i64>,
    /// Fields kept as they are when metadata is matched again.
    pub locked_fields: Vec<String>,
    /// Movies and shows can be matched to another TMDB entry.
    pub can_match: bool,
}

/// Fields to change. Every field sent is stored and locked; an empty string clears an
/// optional field. `lockedFields`, when sent, replaces the lock list before that.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItemMetadata {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    /// 0 clears the year.
    pub year: Option<i64>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    pub content_rating: Option<String>,
    pub studio: Option<String>,
    pub genres: Option<Vec<String>>,
    pub locked_fields: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchItemRequest {
    pub tmdb_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkCandidate {
    /// Full-size image stored when chosen.
    pub url: String,
    /// Smaller image for the picker.
    pub preview_url: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkCandidates {
    /// A TMDB key is configured and the item has a TMDB ID.
    pub available: bool,
    pub current: Option<String>,
    pub candidates: Vec<ArtworkCandidate>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChooseArtwork {
    /// `poster` or `backdrop`.
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct MetadataEditService {
    db: Db,
    jobs: JobQueue,
    events: EventBus,
    tmdb_base: String,
}

impl MetadataEditService {
    pub fn new(db: Db, jobs: JobQueue, events: EventBus) -> Self {
        Self { db, jobs, events, tmdb_base: "https://api.themoviedb.org/3".into() }
    }

    /// Point TMDB requests elsewhere (tests).
    pub fn with_tmdb_base(mut self, base: impl Into<String>) -> Self {
        self.tmdb_base = base.into();
        self
    }

    async fn row(&self, id: ItemId) -> Result<EditableRow> {
        metadata::editable(&self.db, id).await?.ok_or(Error::NotFound)
    }

    pub async fn get(&self, actor: &Actor, id: ItemId) -> Result<ItemMetadata> {
        actor.require_admin()?;
        let row = self.row(id).await?;
        let genres = metadata::genres(&self.db, id).await?;
        Ok(view(id, row, genres))
    }

    pub async fn update(
        &self,
        actor: &Actor,
        id: ItemId,
        req: UpdateItemMetadata,
    ) -> Result<ItemMetadata> {
        actor.require_admin()?;
        let mut row = self.row(id).await?;
        let mut locks = match &req.locked_fields {
            Some(fields) => {
                for f in fields {
                    if !LOCKABLE.contains(&f.as_str()) {
                        return Err(Error::Validation(format!("'{f}' cannot be locked")));
                    }
                }
                fields.clone()
            }
            None => parse_locks(&row.locked_fields),
        };
        let mut lock = |field: &str| {
            if !locks.iter().any(|l| l == field) {
                locks.push(field.to_owned());
            }
        };

        if let Some(title) = req.title {
            let title = text(&title, 500, "Title")?
                .ok_or_else(|| Error::Validation("Title cannot be empty".into()))?;
            if req.sort_title.is_none() {
                row.sort_title = metadata::sort_title(&title);
            }
            row.title = title;
            lock("title");
        }
        if let Some(sort) = req.sort_title {
            row.sort_title = text(&sort, 500, "Sort title")?
                .unwrap_or_else(|| metadata::sort_title(&row.title))
                .to_lowercase();
        }
        if let Some(v) = req.original_title {
            row.original_title = text(&v, 500, "Original title")?;
            lock("original_title");
        }
        if let Some(year) = req.year {
            row.year = match year {
                0 => None,
                1800..=2200 => Some(year),
                _ => return Err(Error::Validation("Year must be between 1800 and 2200".into())),
            };
            lock("year");
        }
        if let Some(v) = req.summary {
            row.summary = text(&v, 20_000, "Summary")?;
            lock("summary");
        }
        if let Some(v) = req.tagline {
            row.tagline = text(&v, 500, "Tagline")?;
            lock("tagline");
        }
        if let Some(v) = req.content_rating {
            row.content_rating = text(&v, 16, "Content rating")?;
            lock("content_rating");
        }
        if let Some(v) = req.studio {
            row.studio = text(&v, 200, "Studio")?;
            lock("studio");
        }
        let genres = match req.genres {
            Some(list) => {
                let mut clean: Vec<String> = Vec::new();
                for g in list {
                    let Some(g) = text(&g, 60, "Genre")? else { continue };
                    if !clean.iter().any(|c| c.eq_ignore_ascii_case(&g)) {
                        clean.push(g);
                    }
                }
                if clean.len() > 30 {
                    return Err(Error::Validation("At most 30 genres".into()));
                }
                lock("genres");
                Some(clean)
            }
            None => None,
        };
        row.locked_fields = serde_json::to_string(&locks).map_err(Error::internal)?;
        metadata::save_edit(&self.db, id, &row, genres.as_deref(), now_ms()).await?;
        tracing::info!(item_id = id.0, "metadata edited");
        self.events.publish(Event::ItemUpdated { item_id: id });
        self.get(actor, id).await
    }

    /// Match a movie or show to the given TMDB entry. Locked fields are kept.
    pub async fn rematch(&self, actor: &Actor, id: ItemId, tmdb_id: i64) -> Result<()> {
        actor.require_admin()?;
        let row = self.row(id).await?;
        if !matches!(row.kind, MediaKind::Movie | MediaKind::Show) {
            return Err(Error::Validation("Only movies and shows can be matched".into()));
        }
        if tmdb_id <= 0 {
            return Err(Error::Validation("Invalid TMDB ID".into()));
        }
        metadata::set_tmdb_id(&self.db, id, tmdb_id, now_ms()).await?;
        self.jobs.enqueue(kinds::METADATA_MATCH, &json!({ "itemId": id }), Priority::High).await?;
        tracing::info!(item_id = id.0, "metadata match queued");
        Ok(())
    }

    /// Poster or backdrop choices from TMDB, plus the current one.
    pub async fn artwork_candidates(
        &self,
        actor: &Actor,
        id: ItemId,
        kind: &str,
    ) -> Result<ArtworkCandidates> {
        actor.require_admin()?;
        let kind = artwork_kind(kind)?;
        let row = self.row(id).await?;
        let current = metadata::artwork_urls(&self.db, id, kind).await?.into_iter().next();
        let key = SettingsRepo::new(self.db.clone())
            .get("tmdb_api_key")
            .await?
            .filter(|k| !k.trim().is_empty());
        let path = match row.kind {
            MediaKind::Movie => "movie",
            MediaKind::Show => "tv",
            _ => "",
        };
        let (Some(key), Some(tmdb_id), false) = (key, row.tmdb_id, path.is_empty()) else {
            return Ok(ArtworkCandidates { available: false, current, candidates: Vec::new() });
        };
        let candidates = self.tmdb_images(&key, path, tmdb_id, kind).await?;
        Ok(ArtworkCandidates { available: true, current, candidates })
    }

    async fn tmdb_images(
        &self,
        key: &str,
        path: &str,
        tmdb_id: i64,
        kind: &str,
    ) -> Result<Vec<ArtworkCandidate>> {
        #[derive(Deserialize)]
        struct Image {
            file_path: String,
            width: Option<i64>,
            height: Option<i64>,
            iso_639_1: Option<String>,
        }
        #[derive(Deserialize, Default)]
        struct Images {
            #[serde(default)]
            posters: Vec<Image>,
            #[serde(default)]
            backdrops: Vec<Image>,
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(Error::internal)?;
        let url = format!("{}/{path}/{tmdb_id}/images", self.tmdb_base.trim_end_matches('/'));
        let mut req = http.get(url).query(&[("include_image_language", "en,sv,null")]);
        req = if key.len() > 40 { req.bearer_auth(key) } else { req.query(&[("api_key", key)]) };
        let resp = req
            .send()
            .await
            .map_err(|e| Error::External(format!("Could not reach TMDB: {}", e.without_url())))?;
        if !resp.status().is_success() {
            return Err(Error::External(format!("TMDB answered {}", resp.status().as_u16())));
        }
        let images: Images = resp.json().await.unwrap_or_default();
        let list = if kind == "poster" { images.posters } else { images.backdrops };
        let preview = if kind == "poster" { "w342" } else { "w780" };
        Ok(list
            .into_iter()
            .filter(|i| safe_tmdb_path(&i.file_path))
            .take(40)
            .map(|i| ArtworkCandidate {
                url: format!("{TMDB_IMAGES}original{}", i.file_path),
                preview_url: format!("{TMDB_IMAGES}{preview}{}", i.file_path),
                width: i.width,
                height: i.height,
                language: i.iso_639_1,
            })
            .collect())
    }

    /// Use one image as the poster or backdrop and lock it against automatic matching.
    pub async fn choose_artwork(
        &self,
        actor: &Actor,
        id: ItemId,
        req: ChooseArtwork,
    ) -> Result<()> {
        actor.require_admin()?;
        let kind = artwork_kind(&req.kind)?;
        let row = self.row(id).await?;
        let url = req.url.trim();
        let current = metadata::artwork_urls(&self.db, id, kind).await?;
        // The server downloads this URL, so only TMDB image paths (or what is already there).
        let allowed =
            url.strip_prefix(TMDB_IMAGES).and_then(|rest| rest.split_once('/')).is_some_and(
                |(size, path)| {
                    size.bytes().all(|b| b.is_ascii_alphanumeric())
                        && safe_tmdb_path(&format!("/{path}"))
                },
            ) || current.iter().any(|c| c == url);
        if !allowed {
            return Err(Error::Validation("Choose an image from the list".into()));
        }
        let mut locks = parse_locks(&row.locked_fields);
        if !locks.iter().any(|l| l == kind) {
            locks.push(kind.to_owned());
        }
        let locked = serde_json::to_string(&locks).map_err(Error::internal)?;
        metadata::choose_artwork(&self.db, id, kind, url, &locked).await?;
        self.jobs.enqueue(kinds::ARTWORK_FETCH, &json!({ "itemId": id }), Priority::High).await?;
        tracing::info!(item_id = id.0, "artwork chosen");
        self.events.publish(Event::ItemUpdated { item_id: id });
        Ok(())
    }
}

fn view(id: ItemId, row: EditableRow, genres: Vec<String>) -> ItemMetadata {
    ItemMetadata {
        item_id: id,
        can_match: matches!(row.kind, MediaKind::Movie | MediaKind::Show),
        kind: row.kind,
        locked_fields: parse_locks(&row.locked_fields),
        title: row.title,
        sort_title: row.sort_title,
        original_title: row.original_title,
        year: row.year,
        summary: row.summary,
        tagline: row.tagline,
        content_rating: row.content_rating,
        studio: row.studio,
        genres,
        tmdb_id: row.tmdb_id,
    }
}

fn parse_locks(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Trimmed text, `None` when empty; too long is refused.
fn text(value: &str, max: usize, what: &str) -> Result<Option<String>> {
    let v = value.trim();
    if v.chars().count() > max {
        return Err(Error::Validation(format!("{what} is too long (at most {max} characters)")));
    }
    Ok((!v.is_empty()).then(|| v.to_owned()))
}

fn artwork_kind(kind: &str) -> Result<&'static str> {
    match kind {
        "poster" => Ok("poster"),
        "backdrop" => Ok("backdrop"),
        _ => Err(Error::Validation("Artwork kind must be poster or backdrop".into())),
    }
}

/// TMDB file paths look like `/abc123XYZ.jpg`.
fn safe_tmdb_path(path: &str) -> bool {
    path.strip_prefix('/').is_some_and(|name| {
        !name.is_empty()
            && name.len() <= 100
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
            && !name.contains("..")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_paths_and_text() {
        assert!(safe_tmdb_path("/kqjL17yufvn9OVLyXYpvtyrFfak.jpg"));
        for bad in ["", "/", "kq.jpg", "/../etc", "/a/b.jpg", "/a?b.jpg"] {
            assert!(!safe_tmdb_path(bad), "{bad}");
        }
        assert_eq!(text("  x ", 5, "T").unwrap().as_deref(), Some("x"));
        assert_eq!(text("   ", 5, "T").unwrap(), None);
        assert!(text("toolong", 3, "T").is_err());
        assert!(artwork_kind("logo").is_err());
    }
}
