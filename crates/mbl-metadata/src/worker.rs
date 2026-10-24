//! Matching items against local NFO files and TMDB, and the `metadata.*` jobs.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use mbl_core::events::{Event, EventBus};
use mbl_core::{Error, ItemId, LibraryId, MediaKind, Result, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::metadata::{self, MatchTarget, MetadataUpdate, PersonCredit};
use mbl_jobs::{JobCtx, JobHandler, JobQueue, Priority, kinds};
use serde_json::json;

use crate::nfo::{find_local_artwork, read_first_nfo};
use crate::{Candidate, MetadataBundle, TmdbClient, score};

/// Minimum confidence for automatically accepting a search result.
pub const ACCEPT: f64 = 0.85;

#[derive(Debug, Clone)]
pub struct Matcher {
    db: Db,
    tmdb: Option<TmdbClient>,
    events: EventBus,
}

impl Matcher {
    pub fn new(db: Db, tmdb: Option<TmdbClient>, events: EventBus) -> Self {
        if tmdb.is_none() {
            tracing::warn!("no TMDB API key configured: only local NFO metadata will be used");
        }
        Self { db, tmdb, events }
    }

    /// Match one top-level item (movie or show) and store the result.
    pub async fn match_item(&self, id: ItemId) -> Result<()> {
        let target = metadata::match_target(&self.db, id).await?.ok_or(Error::NotFound)?;
        match target.kind {
            MediaKind::Movie => self.match_movie(&target).await,
            MediaKind::Show => self.match_show(&target).await,
            // Music metadata (MusicBrainz) arrives in M7.1.
            _ => Ok(()),
        }
    }

    async fn local(&self, id: ItemId, show: bool) -> Result<MetadataBundle> {
        let Some(file) = metadata::first_file(&self.db, id).await? else {
            return Ok(MetadataBundle::default());
        };
        tokio::task::spawn_blocking(move || local_metadata(Path::new(&file), show))
            .await
            .map_err(Error::internal)
    }

    async fn match_movie(&self, t: &MatchTarget) -> Result<()> {
        let local = self.local(t.id, false).await?;
        let (online, confidence) = match &self.tmdb {
            None => (MetadataBundle::default(), None),
            Some(c) => match local.tmdb_id.or(t.tmdb_id) {
                Some(id) => (c.movie(id).await?, Some(1.0)),
                None => {
                    let title = local.title.clone().unwrap_or_else(|| t.title.clone());
                    let year = local.year.or(t.year.and_then(|y| i32::try_from(y).ok()));
                    let found = c.search_movie(&title, year).await?;
                    match best(&title, year, &found) {
                        Some((cand, s)) if s >= ACCEPT => (c.movie(cand.tmdb_id).await?, Some(s)),
                        other => (MetadataBundle::default(), Some(other.map_or(0.0, |(_, s)| s))),
                    }
                }
            },
        };
        self.store(t, local.or(online), confidence).await
    }

    async fn match_show(&self, t: &MatchTarget) -> Result<()> {
        let local = self.local(t.id, true).await?;
        let Some(client) = &self.tmdb else {
            return self.store(t, local, None).await;
        };
        let (online, confidence) = match local.tmdb_id.or(t.tmdb_id) {
            Some(id) => (client.tv(id).await?, Some(1.0)),
            None => {
                let title = local.title.clone().unwrap_or_else(|| t.title.clone());
                let year = local.year.or(t.year.and_then(|y| i32::try_from(y).ok()));
                let found = client.search_tv(&title, year).await?;
                match best(&title, year, &found) {
                    Some((cand, s)) if s >= ACCEPT => (client.tv(cand.tmdb_id).await?, Some(s)),
                    other => (MetadataBundle::default(), Some(other.map_or(0.0, |(_, s)| s))),
                }
            }
        };
        let tmdb_id = online.tmdb_id;
        self.store(t, local.or(online), confidence).await?;
        if let Some(tv_id) = tmdb_id {
            self.match_episodes(client, t.id, tv_id).await?;
        }
        Ok(())
    }

    async fn match_episodes(&self, client: &TmdbClient, show: ItemId, tv_id: i64) -> Result<()> {
        for season in metadata::children(&self.db, show).await? {
            let Some(number) = season.idx.and_then(|n| u32::try_from(n).ok()) else { continue };
            let data = match client.season(tv_id, number).await {
                Ok(d) => d,
                Err(Error::NotFound) => continue,
                Err(e) => return Err(e),
            };
            let episodes = metadata::children(&self.db, season.id).await?;
            let now = now_ms();
            let mut tx = self.db.write.begin().await?;
            let season_update =
                MetadataUpdate { summary: data.summary.clone(), ..Default::default() };
            metadata::apply(
                &mut tx,
                season.id,
                &season_update.without_locked(&season.locked_fields),
                now,
            )
            .await?;
            if let Some(poster) = &data.poster {
                metadata::replace_artwork_urls(
                    &mut tx,
                    season.id,
                    "poster",
                    std::slice::from_ref(poster),
                )
                .await?;
            }
            for ep in &episodes {
                let Some(src) = data.episodes.iter().find(|e| ep.idx == Some(i64::from(e.number)))
                else {
                    continue;
                };
                let update = MetadataUpdate {
                    title: src.title.clone(),
                    summary: src.summary.clone(),
                    originally_available_at: src.air_date.clone(),
                    rating: src.rating,
                    ..Default::default()
                };
                metadata::apply(&mut tx, ep.id, &update.without_locked(&ep.locked_fields), now)
                    .await?;
                if let Some(still) = &src.still {
                    metadata::replace_artwork_urls(
                        &mut tx,
                        ep.id,
                        "thumb",
                        std::slice::from_ref(still),
                    )
                    .await?;
                }
            }
            tx.commit().await?;
        }
        Ok(())
    }

    async fn store(
        &self,
        t: &MatchTarget,
        b: MetadataBundle,
        confidence: Option<f64>,
    ) -> Result<()> {
        let update = MetadataUpdate {
            title: b.title,
            original_title: b.original_title,
            summary: b.summary,
            tagline: b.tagline,
            year: b.year.map(i64::from),
            originally_available_at: b.release_date,
            content_rating: None,
            studio: b.studio,
            rating: b.rating,
            tmdb_id: b.tmdb_id,
            tvdb_id: b.tvdb_id,
            imdb_id: b.imdb_id,
            match_confidence: confidence,
            needs_review: confidence.map(|c| c < ACCEPT),
        }
        .without_locked(&t.locked_fields);

        let mut tx = self.db.write.begin().await?;
        metadata::apply(&mut tx, t.id, &update, now_ms()).await?;
        if !b.people.is_empty() {
            let people: Vec<PersonCredit> = b.people.iter().map(Into::into).collect();
            metadata::replace_people(&mut tx, t.id, &people).await?;
        }
        if !b.genres.is_empty() {
            metadata::replace_tags(&mut tx, t.id, "genre", &b.genres).await?;
        }
        if !b.posters.is_empty() {
            metadata::replace_artwork_urls(&mut tx, t.id, "poster", &b.posters).await?;
        }
        if !b.backdrops.is_empty() {
            metadata::replace_artwork_urls(&mut tx, t.id, "backdrop", &b.backdrops).await?;
        }
        tx.commit().await?;
        tracing::debug!(item_id = %t.id, ?confidence, "metadata stored");
        self.events.publish(Event::ItemUpdated { item_id: t.id });
        Ok(())
    }
}

fn best<'a>(
    title: &str,
    year: Option<i32>,
    found: &'a [Candidate],
) -> Option<(&'a Candidate, f64)> {
    found.iter().map(|c| (c, score(title, year, c))).max_by(|a, b| a.1.total_cmp(&b.1))
}

/// NFO + local artwork for a movie file, or for the show folder above an episode file.
fn local_metadata(file: &Path, show: bool) -> MetadataBundle {
    let Some(dir) = file.parent() else { return MetadataBundle::default() };
    let stem = file.file_stem().and_then(|s| s.to_str());
    if !show {
        let nfos: Vec<PathBuf> = stem
            .map(|s| dir.join(format!("{s}.nfo")))
            .into_iter()
            .chain([dir.join("movie.nfo")])
            .collect();
        let mut b = read_first_nfo(&nfos).unwrap_or_default();
        let art = find_local_artwork(dir, stem);
        b.posters = art.posters;
        b.backdrops = art.backdrops;
        return b;
    }
    // Episode file → season folder → show folder; use the first level that has a tvshow.nfo or art.
    for level in dir.ancestors().take(3) {
        let nfo = read_first_nfo(&[level.join("tvshow.nfo")]);
        let art = find_local_artwork(level, None);
        if nfo.is_some() || !art.posters.is_empty() || !art.backdrops.is_empty() {
            let mut b = nfo.unwrap_or_default();
            b.posters = art.posters;
            b.backdrops = art.backdrops;
            return b;
        }
    }
    MetadataBundle::default()
}

/// `metadata.match` job: payload `{"itemId": n}`. Queues `artwork.fetch` afterwards.
pub struct MatchHandler {
    pub matcher: Matcher,
    pub jobs: Option<JobQueue>,
}

#[async_trait]
impl JobHandler for MatchHandler {
    fn kind(&self) -> &'static str {
        kinds::METADATA_MATCH
    }

    fn concurrency(&self, _: mbl_platform::PlatformProfile) -> usize {
        2
    }

    async fn run(&self, _ctx: JobCtx, payload: serde_json::Value) -> Result<()> {
        let id = payload["itemId"]
            .as_i64()
            .map(ItemId)
            .ok_or_else(|| Error::Validation("metadata.match without itemId".into()))?;
        match self.matcher.match_item(id).await {
            // The item was removed since the job was queued.
            Err(Error::NotFound) => Ok(()),
            Err(e) => Err(e),
            Ok(()) => {
                if let Some(jobs) = &self.jobs {
                    jobs.enqueue(kinds::ARTWORK_FETCH, &json!({ "itemId": id }), Priority::Idle)
                        .await?;
                }
                Ok(())
            }
        }
    }
}

/// `metadata.refresh` job: payload `{"libraryId": n}` or `{"itemId": n}`; re-queues matching.
pub struct RefreshHandler {
    pub db: Db,
    pub jobs: JobQueue,
}

#[async_trait]
impl JobHandler for RefreshHandler {
    fn kind(&self) -> &'static str {
        kinds::METADATA_REFRESH
    }

    async fn run(&self, _ctx: JobCtx, payload: serde_json::Value) -> Result<()> {
        let ids = match (payload["itemId"].as_i64(), payload["libraryId"].as_i64()) {
            (Some(item), _) => vec![ItemId(item)],
            (None, Some(lib)) => metadata::top_level_items(&self.db, LibraryId(lib)).await?,
            _ => {
                return Err(Error::Validation("metadata.refresh needs itemId or libraryId".into()));
            }
        };
        for id in ids {
            self.jobs
                .enqueue(kinds::METADATA_MATCH, &json!({ "itemId": id }), Priority::Normal)
                .await?;
        }
        Ok(())
    }
}
