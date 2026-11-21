//! What is missing from the library, compared with TMDB: episodes (gaps, after the last one you
//! have and whole seasons) and movies from collections such as trilogies.
//!
//! Only aired episodes and released movies count; specials (season 0) are left out. TMDB answers
//! are kept in memory for 12 hours, so the overview of a large library is quick the second time.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_util::StreamExt;
use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, ItemId, LibraryId, MediaKind, Result};
use mbl_db::Db;
use mbl_db::repo::items::{self, ItemFilter};
use mbl_metadata::{TmdbClient, TmdbCollectionPart, TmdbConfig, TmdbShowOverview};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utoipa::ToSchema;

const CACHE_MS: i64 = 12 * 3600 * 1000;
/// Titles checked in one library overview.
const MAX_SHOWS: usize = 300;
const MAX_MOVIES: usize = 600;
const PARALLEL: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MissingEpisode {
    pub episode: u32,
    pub title: Option<String>,
    pub air_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MissingSeason {
    pub season: u32,
    pub name: Option<String>,
    /// No episode of this season is in the library.
    pub whole: bool,
    pub episodes: Vec<MissingEpisode>,
}

/// Missing episodes of one show.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShowMissing {
    pub show_id: ItemId,
    pub title: String,
    pub tmdb_id: Option<i64>,
    pub missing_count: u32,
    pub seasons: Vec<MissingSeason>,
}

/// A released movie from a collection that the library lacks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MissingMovie {
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub collection: String,
    pub poster: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMissing {
    pub library_id: LibraryId,
    /// Shows with missing episodes (season lists without episode titles).
    pub shows: Vec<ShowMissing>,
    pub movies: Vec<MissingMovie>,
    /// Titles compared with TMDB.
    pub checked: u32,
    /// Titles without a TMDB ID, which cannot be compared.
    pub without_tmdb: u32,
    /// The library had more titles than one overview checks.
    pub truncated: bool,
}

/// Missing aired episodes per season, given what the library has.
pub fn missing_in_show(
    overview: &TmdbShowOverview,
    have: &HashSet<(u32, u32)>,
) -> Vec<MissingSeason> {
    let Some((last_season, last_episode)) = overview.last_aired else { return Vec::new() };
    let mut seasons: Vec<_> = overview.seasons.iter().filter(|s| s.number > 0).collect();
    seasons.sort_by_key(|s| s.number);
    seasons
        .into_iter()
        .filter_map(|s| {
            let aired = match s.number.cmp(&last_season) {
                std::cmp::Ordering::Less => s.episode_count,
                std::cmp::Ordering::Equal => last_episode,
                std::cmp::Ordering::Greater => 0,
            };
            let episodes: Vec<MissingEpisode> = (1..=aired)
                .filter(|e| !have.contains(&(s.number, *e)))
                .map(|episode| MissingEpisode { episode, title: None, air_date: None })
                .collect();
            if episodes.is_empty() {
                return None;
            }
            let whole = !have.iter().any(|(season, _)| *season == s.number);
            Some(MissingSeason { season: s.number, name: s.name.clone(), whole, episodes })
        })
        .collect()
}

/// `YYYY-MM-DD` of a release is on or before `today`.
fn released(date: Option<&str>, today: &str) -> bool {
    date.is_some_and(|d| d.len() >= 10 && &d[..10] <= today)
}

#[derive(Default)]
struct Cache {
    shows: HashMap<i64, (i64, TmdbShowOverview)>,
    movie_collection: HashMap<i64, (i64, Option<(i64, String)>)>,
    collections: HashMap<i64, (i64, Vec<TmdbCollectionPart>)>,
}

#[derive(Clone)]
pub struct MissingService {
    db: Db,
    cache: Arc<Mutex<Cache>>,
    client: Arc<Mutex<Option<(String, TmdbClient)>>>,
}

impl std::fmt::Debug for MissingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MissingService").finish_non_exhaustive()
    }
}

fn fresh(at: i64) -> bool {
    now_ms() - at < CACHE_MS
}

impl MissingService {
    pub fn new(db: Db) -> Self {
        Self { db, cache: Arc::default(), client: Arc::default() }
    }

    /// TMDB client for the stored key (and test address), reused while the key is the same.
    async fn tmdb(&self) -> Result<TmdbClient> {
        let settings = mbl_db::repo::settings::SettingsRepo::new(self.db.clone());
        let key = settings.get("tmdb_api_key").await?.unwrap_or_default().trim().to_owned();
        if key.is_empty() {
            return Err(Error::Validation(
                "Finding what is missing needs a TMDB API key under Server settings".into(),
            ));
        }
        let mut lock = self.client.lock().await;
        if let Some((cached, client)) = lock.as_ref()
            && *cached == key
        {
            return Ok(client.clone());
        }
        let mut config = TmdbConfig::new(key.clone(), "en-US");
        if let Some(base) = settings.get("tmdb_base_url").await?.filter(|b| !b.is_empty()) {
            config.base_url = base;
        }
        let client = TmdbClient::new(config)?;
        *lock = Some((key, client.clone()));
        Ok(client)
    }

    async fn overview(&self, tmdb: &TmdbClient, id: i64) -> Result<TmdbShowOverview> {
        if let Some((at, o)) = self.cache.lock().await.shows.get(&id)
            && fresh(*at)
        {
            return Ok(o.clone());
        }
        let o = tmdb.tv_episodes_overview(id).await?;
        self.cache.lock().await.shows.insert(id, (now_ms(), o.clone()));
        Ok(o)
    }

    async fn collection_of(&self, tmdb: &TmdbClient, movie: i64) -> Result<Option<(i64, String)>> {
        if let Some((at, c)) = self.cache.lock().await.movie_collection.get(&movie)
            && fresh(*at)
        {
            return Ok(c.clone());
        }
        let c = tmdb.movie_collection(movie).await?;
        self.cache.lock().await.movie_collection.insert(movie, (now_ms(), c.clone()));
        Ok(c)
    }

    async fn parts(&self, tmdb: &TmdbClient, collection: i64) -> Result<Vec<TmdbCollectionPart>> {
        if let Some((at, p)) = self.cache.lock().await.collections.get(&collection)
            && fresh(*at)
        {
            return Ok(p.clone());
        }
        let p = tmdb.collection_parts(collection).await?;
        self.cache.lock().await.collections.insert(collection, (now_ms(), p.clone()));
        Ok(p)
    }

    async fn have(&self, show: ItemId) -> Result<HashSet<(u32, u32)>> {
        Ok(items::episode_numbers(&self.db, show)
            .await?
            .into_iter()
            .filter_map(|(s, e)| Some((u32::try_from(s).ok()?, u32::try_from(e).ok()?)))
            .collect())
    }

    /// Missing episodes of one show, with episode titles.
    pub async fn show(&self, actor: &Actor, show: ItemId) -> Result<ShowMissing> {
        let detail = crate::ItemService::new(self.db.clone()).get(actor, show).await?;
        if detail.kind != MediaKind::Show {
            return Err(Error::Validation("Only shows have episodes".into()));
        }
        let mut result = ShowMissing {
            show_id: show,
            title: detail.title,
            tmdb_id: detail.tmdb_id,
            missing_count: 0,
            seasons: Vec::new(),
        };
        let Some(tmdb_id) = detail.tmdb_id else { return Ok(result) };
        let tmdb = self.tmdb().await?;
        let overview = self.overview(&tmdb, tmdb_id).await?;
        let mut seasons = missing_in_show(&overview, &self.have(show).await?);
        for season in &mut seasons {
            if let Ok(info) = tmdb.season(tmdb_id, season.season).await {
                for missing in &mut season.episodes {
                    if let Some(e) = info.episodes.iter().find(|e| e.number == missing.episode) {
                        missing.title = e.title.clone();
                        missing.air_date = e.air_date.clone();
                    }
                }
            }
        }
        result.missing_count = seasons.iter().map(|s| s.episodes.len() as u32).sum();
        result.seasons = seasons;
        Ok(result)
    }

    /// What a movie or TV library is missing.
    pub async fn library(&self, actor: &Actor, library: LibraryId) -> Result<LibraryMissing> {
        let filter = ItemFilter::for_user(actor);
        let lib = mbl_db::repo::libraries::LibrariesRepo::new(self.db.clone())
            .get(library)
            .await?
            .filter(|_| filter.allows(library))
            .ok_or(Error::NotFound)?;
        let tmdb = self.tmdb().await?;
        let mut out = LibraryMissing {
            library_id: library,
            shows: Vec::new(),
            movies: Vec::new(),
            checked: 0,
            without_tmdb: 0,
            truncated: false,
        };
        let kind = match lib.kind {
            mbl_core::LibraryKind::Show => "show",
            mbl_core::LibraryKind::Movie => "movie",
            _ => return Ok(out),
        };
        let titles: Vec<_> = items::top_level_with_tmdb(&self.db, library, kind)
            .await?
            .into_iter()
            .filter(|t| filter.allows_rating(t.content_rating.as_deref()))
            .collect();
        out.without_tmdb = titles.iter().filter(|t| t.tmdb_id.is_none()).count() as u32;
        let limit = if kind == "show" { MAX_SHOWS } else { MAX_MOVIES };
        // Owned (id, title, TMDB ID) so the parallel lookups own what they use.
        let with_ids: Vec<(i64, String, i64)> =
            titles.iter().filter_map(|t| Some((t.id, t.title.clone(), t.tmdb_id?))).collect();
        out.truncated = with_ids.len() > limit;
        let with_ids: Vec<_> = with_ids.into_iter().take(limit).collect();
        out.checked = with_ids.len() as u32;

        if kind == "show" {
            let found: Vec<Option<ShowMissing>> = futures_util::stream::iter(with_ids)
                .map(|(id, title, tmdb_id)| {
                    let (service, tmdb) = (self.clone(), tmdb.clone());
                    async move {
                        let overview = service.overview(&tmdb, tmdb_id).await.ok()?;
                        let have = service.have(ItemId(id)).await.ok()?;
                        let seasons = missing_in_show(&overview, &have);
                        let missing_count = seasons.iter().map(|s| s.episodes.len() as u32).sum();
                        (missing_count > 0).then_some(ShowMissing {
                            show_id: ItemId(id),
                            title,
                            tmdb_id: Some(tmdb_id),
                            missing_count,
                            seasons,
                        })
                    }
                })
                .buffered(PARALLEL)
                .collect()
                .await;
            out.shows = found.into_iter().flatten().collect();
        } else {
            let have: HashSet<i64> = titles.iter().filter_map(|t| t.tmdb_id).collect();
            let collections: Vec<Option<(i64, String)>> = futures_util::stream::iter(with_ids)
                .map(|(_, _, tmdb_id)| {
                    let (service, tmdb) = (self.clone(), tmdb.clone());
                    async move { service.collection_of(&tmdb, tmdb_id).await.ok().flatten() }
                })
                .buffered(PARALLEL)
                .collect()
                .await;
            let mut seen = HashSet::new();
            let today = time::OffsetDateTime::now_utc().date().to_string();
            for (id, name) in collections.into_iter().flatten() {
                if !seen.insert(id) {
                    continue;
                }
                let Ok(parts) = self.parts(&tmdb, id).await else { continue };
                for part in parts {
                    if have.contains(&part.tmdb_id)
                        || !released(part.release_date.as_deref(), &today)
                    {
                        continue;
                    }
                    out.movies.push(MissingMovie {
                        tmdb_id: part.tmdb_id,
                        title: part.title,
                        year: part.release_date.as_deref().and_then(|d| d.get(..4)?.parse().ok()),
                        collection: name.clone(),
                        poster: part.poster,
                    });
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbl_metadata::TmdbSeasonSummary;

    fn season(number: u32, episode_count: u32) -> TmdbSeasonSummary {
        TmdbSeasonSummary { number, episode_count, name: Some(format!("Season {number}")) }
    }

    #[test]
    fn gaps_later_episodes_and_whole_seasons_but_not_specials_or_the_future() {
        let overview = TmdbShowOverview {
            seasons: vec![season(0, 5), season(1, 8), season(2, 10), season(3, 10), season(4, 8)],
            last_aired: Some((3, 4)),
        };
        // Season 1: a gap (5) and the end (7–8). Season 2: nothing. Season 3: aired up to 4.
        let have: HashSet<(u32, u32)> =
            [(1, 1), (1, 2), (1, 3), (1, 4), (1, 6), (3, 1)].into_iter().collect();
        let missing = missing_in_show(&overview, &have);
        let summary: Vec<(u32, bool, Vec<u32>)> = missing
            .iter()
            .map(|s| (s.season, s.whole, s.episodes.iter().map(|e| e.episode).collect()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (1, false, vec![5, 7, 8]),
                (2, true, (1..=10).collect()),
                (3, false, vec![2, 3, 4]),
            ]
        );
        // Nothing has aired: nothing is missing.
        let unaired = TmdbShowOverview { seasons: vec![season(1, 8)], last_aired: None };
        assert!(missing_in_show(&unaired, &HashSet::new()).is_empty());
    }

    #[test]
    fn only_released_movies_count() {
        assert!(released(Some("2024-05-01"), "2026-11-21"));
        assert!(released(Some("2026-11-21"), "2026-11-21"));
        assert!(!released(Some("2027-01-01"), "2026-11-21"));
        assert!(!released(None, "2026-11-21"));
    }
}
