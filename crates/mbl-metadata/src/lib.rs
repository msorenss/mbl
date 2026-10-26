//! Metadata agents (local NFO, TMDB, ...) and matching.
//!
//! The agent chain for movies and shows is Local → TMDB: values from local NFO files win, TMDB
//! fills the rest. Locked fields are never overwritten.

mod matcher;
pub mod musicbrainz;
mod nfo;
pub mod opensubtitles;
mod tmdb;
mod worker;

use mbl_db::repo::metadata::PersonCredit;

pub use matcher::{normalize_title, score};
pub use musicbrainz::{MbArtist, MbRelease, MbReleaseDetails, MbTrack, MusicBrainzClient};
pub use nfo::{LocalArtwork, find_local_artwork, parse_nfo};
pub use opensubtitles::{OpenSubtitlesClient, SubtitleCandidate};
pub use tmdb::{TmdbClient, TmdbConfig};
pub use worker::{MatchHandler, Matcher, RefreshHandler};

/// Everything an agent knows about one item. Fields left `None`/empty are filled by later agents.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetadataBundle {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    pub year: Option<i32>,
    pub release_date: Option<String>,
    pub rating: Option<f64>,
    pub studio: Option<String>,
    pub genres: Vec<String>,
    pub people: Vec<Credit>,
    pub posters: Vec<String>,
    pub backdrops: Vec<String>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    pub imdb_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credit {
    pub name: String,
    pub tmdb_id: Option<i64>,
    pub thumb_url: Option<String>,
    /// `actor`, `director` or `writer`.
    pub role: String,
    pub character: Option<String>,
}

impl From<&Credit> for PersonCredit {
    fn from(c: &Credit) -> Self {
        PersonCredit {
            name: c.name.clone(),
            tmdb_id: c.tmdb_id,
            thumb_url: c.thumb_url.clone(),
            role: c.role.clone(),
            character: c.character.clone(),
        }
    }
}

impl MetadataBundle {
    /// Keep our values, fill gaps from `other` (agent-chain merge).
    pub fn or(self, other: MetadataBundle) -> MetadataBundle {
        fn pick<T>(a: Vec<T>, b: Vec<T>) -> Vec<T> {
            if a.is_empty() { b } else { a }
        }
        MetadataBundle {
            title: self.title.or(other.title),
            original_title: self.original_title.or(other.original_title),
            summary: self.summary.or(other.summary),
            tagline: self.tagline.or(other.tagline),
            year: self.year.or(other.year),
            release_date: self.release_date.or(other.release_date),
            rating: self.rating.or(other.rating),
            studio: self.studio.or(other.studio),
            genres: pick(self.genres, other.genres),
            people: pick(self.people, other.people),
            posters: [self.posters, other.posters].concat(),
            backdrops: [self.backdrops, other.backdrops].concat(),
            tmdb_id: self.tmdb_id.or(other.tmdb_id),
            tvdb_id: self.tvdb_id.or(other.tvdb_id),
            imdb_id: self.imdb_id.or(other.imdb_id),
        }
    }
}

/// A search hit from an online agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub tmdb_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i32>,
}
