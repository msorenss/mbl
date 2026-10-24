//! Filename and path parsing for movies, episodes and music.
//!
//! Pure functions, no I/O. Paths may use `/` or `\` separators and may be absolute or relative to
//! a library root; only the last few components are looked at.

mod common;
mod episode;
mod extra;
mod movie;
mod music;

pub use episode::parse_episode;
pub use extra::{ExtraKind, classify_extra, is_sample};
pub use movie::parse_movie;
pub use music::parse_music;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExternalIds {
    pub tmdb: Option<i64>,
    pub tvdb: Option<i64>,
    pub imdb: Option<String>,
}

impl ExternalIds {
    /// Fill missing IDs from `other`.
    pub fn or(self, other: ExternalIds) -> ExternalIds {
        ExternalIds {
            tmdb: self.tmdb.or(other.tmdb),
            tvdb: self.tvdb.or(other.tvdb),
            imdb: self.imdb.or(other.imdb),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MovieGuess {
    pub title: String,
    pub year: Option<i32>,
    pub ids: ExternalIds,
    pub edition: Option<String>,
    pub part: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EpisodeGuess {
    pub show: String,
    pub show_year: Option<i32>,
    pub ids: ExternalIds,
    pub season: Option<u32>,
    pub episodes: Vec<u32>,
    pub absolute: Option<u32>,
    pub air_date: Option<time::Date>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MusicGuess {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_year: Option<i32>,
    pub disc: Option<u32>,
    pub track: Option<u32>,
    pub title: Option<String>,
}
