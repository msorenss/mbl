//! Read models for browsing: lists, details, children, search and hubs.
//!
//! Every query takes an [`ItemFilter`] so library ACLs are applied in one place. List queries
//! with user-chosen sorting and filters are built at runtime; fixed-shape lookups use macros.

use mbl_core::{
    Actor, ArtworkId, Cursor, ItemId, LibraryAccess, LibraryId, MediaKind, Paginated, PartId,
    Result, StreamKind, UserId, VersionId,
};
use serde::Serialize;
use sqlx::{QueryBuilder, Sqlite};
use utoipa::ToSchema;

use crate::Db;

pub const MAX_LIMIT: u32 = 200;

/// Which libraries an actor may read, plus whose watch state to join.
#[derive(Debug, Clone)]
pub struct ItemFilter {
    pub user_id: UserId,
    libraries: Option<Vec<LibraryId>>,
    pub content_rating_ceiling: Option<String>,
}

impl ItemFilter {
    pub fn for_user(actor: &Actor) -> Self {
        let libraries = match &actor.library_access {
            LibraryAccess::All => None,
            LibraryAccess::Only(ids) => Some(ids.clone()),
        };
        Self {
            user_id: actor.user_id,
            libraries,
            content_rating_ceiling: actor.content_rating_ceiling.clone(),
        }
    }

    pub fn allows(&self, library: LibraryId) -> bool {
        self.libraries.as_ref().is_none_or(|ids| ids.contains(&library))
    }

    pub fn allows_rating(&self, rating: Option<&str>) -> bool {
        let Some(ceiling) = &self.content_rating_ceiling else {
            return true;
        };
        mbl_core::actor::allows_content_rating(ceiling, rating)
    }

    fn push_acl(&self, qb: &mut QueryBuilder<Sqlite>) {
        if let Some(ids) = &self.libraries {
            qb.push(" AND i.library_id IN (");
            let mut sep = qb.separated(", ");
            sep.push_bind(-1_i64);
            for id in ids {
                sep.push_bind(id.0);
            }
            qb.push(")");
        }
        if let Some(ceiling) = &self.content_rating_ceiling {
            let max_lvl = mbl_core::actor::content_rating_level(ceiling);
            if max_lvl == 0 {
                qb.push(" AND i.content_rating IS NOT NULL AND i.content_rating != '' AND UPPER(REPLACE(REPLACE(REPLACE(i.content_rating, '-', ''), '_', ''), ' ', '')) IN ('G', 'TVY', 'TVG', 'U', 'ALL', 'EC')");
            } else if max_lvl < 4 {
                let mut disallowed = vec!["NC17", "X", "NR", "UNRATED", "AO"];
                if max_lvl < 3 {
                    disallowed.extend_from_slice(&["R", "TVMA", "15", "16", "18", "M"]);
                }
                if max_lvl < 2 {
                    disallowed.extend_from_slice(&["PG13", "TV14", "12", "14", "T"]);
                }
                qb.push(" AND (i.content_rating IS NULL OR i.content_rating = '' OR UPPER(REPLACE(REPLACE(REPLACE(i.content_rating, '-', ''), '_', ''), ' ', '')) NOT IN (");
                let mut sep = qb.separated(", ");
                for d in disallowed {
                    sep.push_bind(d);
                }
                qb.push("))");
            }
        }
    }
}

/// Compact item for grids and rows.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub id: ItemId,
    pub library_id: LibraryId,
    pub kind: MediaKind,
    pub parent_id: Option<ItemId>,
    pub title: String,
    pub year: Option<i64>,
    pub idx: Option<i64>,
    pub parent_idx: Option<i64>,
    pub rating: Option<f64>,
    pub duration_ms: Option<i64>,
    pub added_at: i64,
    pub poster_id: Option<ArtworkId>,
    pub poster_hash: Option<String>,
    pub poster_blurhash: Option<String>,
    pub view_offset_ms: i64,
    pub view_count: i64,
}

const SUMMARY: &str = "SELECT i.id, i.library_id, i.kind, i.parent_id, i.title, i.year, i.idx, \
    i.parent_idx, i.rating, i.duration_ms, i.added_at, a.id AS poster_id, \
    a.local_hash AS poster_hash, a.blurhash AS poster_blurhash, \
    COALESCE(w.view_offset_ms, 0) AS view_offset_ms, COALESCE(w.view_count, 0) AS view_count \
    FROM media_items i \
    LEFT JOIN artwork a ON a.id = (SELECT id FROM artwork WHERE item_id = i.id \
        AND kind = 'poster' ORDER BY selected DESC, local_hash IS NULL, id LIMIT 1) \
    LEFT JOIN watch_state w ON w.item_id = i.id AND w.user_id = ";

fn summary_query(filter: &ItemFilter) -> QueryBuilder<Sqlite> {
    let mut qb = QueryBuilder::new(SUMMARY);
    qb.push_bind(filter.user_id.0);
    qb.push(" WHERE 1 = 1");
    filter.push_acl(&mut qb);
    qb
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    #[default]
    Title,
    Added,
    Year,
    Rating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    /// Defaults to top-level items (no parent).
    pub kind: Option<MediaKind>,
    pub sort: SortField,
    pub order: Option<SortOrder>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub unwatched: bool,
    pub cursor: Option<Cursor>,
    pub limit: u32,
}

/// One page of a library. Cursors are opaque offsets.
pub async fn list(
    db: &Db,
    filter: &ItemFilter,
    library: LibraryId,
    q: &ListQuery,
) -> Result<Paginated<ItemSummary>> {
    let limit = q.limit.clamp(1, MAX_LIMIT);
    let offset = match &q.cursor {
        Some(c) => {
            c.0.parse::<u32>().map_err(|_| mbl_core::Error::Validation("invalid cursor".into()))?
        }
        None => 0,
    };
    let mut qb = summary_query(filter);
    qb.push(" AND i.library_id = ").push_bind(library.0);
    match q.kind {
        Some(kind) => {
            qb.push(" AND i.kind = ").push_bind(kind);
        }
        None => {
            qb.push(" AND i.parent_id IS NULL AND i.kind != 'extra'");
        }
    }
    if let Some(genre) = &q.genre {
        qb.push(
            " AND EXISTS (SELECT 1 FROM item_tags it JOIN tags t ON t.id = it.tag_id \
             WHERE it.item_id = i.id AND t.kind = 'genre' AND t.name = ",
        )
        .push_bind(genre.clone())
        .push(" COLLATE NOCASE)");
    }
    if let Some(year) = q.year {
        qb.push(" AND i.year = ").push_bind(year);
    }
    if q.unwatched {
        qb.push(" AND COALESCE(w.view_count, 0) = 0");
    }
    let (column, default_order) = match q.sort {
        SortField::Title => ("i.sort_title COLLATE NOCASE", SortOrder::Asc),
        SortField::Added => ("i.added_at", SortOrder::Desc),
        SortField::Year => ("i.year", SortOrder::Desc),
        SortField::Rating => ("i.rating", SortOrder::Desc),
    };
    let dir = match q.order.unwrap_or(default_order) {
        SortOrder::Asc => "ASC",
        SortOrder::Desc => "DESC",
    };
    qb.push(format!(" ORDER BY {column} {dir}, i.id {dir} LIMIT "));
    qb.push_bind(i64::from(limit) + 1).push(" OFFSET ").push_bind(i64::from(offset));

    let mut items: Vec<ItemSummary> = qb.build_query_as().fetch_all(&db.read).await?;
    let next_cursor = (items.len() > limit as usize).then(|| {
        items.truncate(limit as usize);
        Cursor((offset + limit).to_string())
    });
    Ok(Paginated { items, next_cursor, total: None })
}

/// Seasons of a show, episodes of a season, albums of an artist, tracks of an album.
pub async fn children(db: &Db, filter: &ItemFilter, parent: ItemId) -> Result<Vec<ItemSummary>> {
    let mut qb = summary_query(filter);
    qb.push(" AND i.parent_id = ").push_bind(parent.0);
    qb.push(" AND i.kind != 'extra' ORDER BY i.idx IS NULL, i.idx, i.sort_title, i.id");
    Ok(qb.build_query_as().fetch_all(&db.read).await?)
}

/// Full-text search over titles; returns at most `limit` hits per kind in rank order.
pub async fn search(
    db: &Db,
    filter: &ItemFilter,
    query: &str,
    limit_per_kind: usize,
) -> Result<Vec<ItemSummary>> {
    let Some(fts) = fts_query(query) else { return Ok(Vec::new()) };
    let ranked: Vec<i64> = sqlx::query_scalar!(
        r#"SELECT rowid AS "id!: i64" FROM items_fts WHERE items_fts MATCH ? ORDER BY rank LIMIT 500"#,
        fts
    )
    .fetch_all(&db.read)
    .await?;
    if ranked.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = summary_query(filter);
    qb.push(" AND i.kind != 'extra' AND i.id IN (");
    let mut sep = qb.separated(", ");
    for id in &ranked {
        sep.push_bind(*id);
    }
    qb.push(")");
    let mut hits: Vec<ItemSummary> = qb.build_query_as().fetch_all(&db.read).await?;
    let pos: std::collections::HashMap<i64, usize> =
        ranked.iter().enumerate().map(|(n, id)| (*id, n)).collect();
    hits.sort_by_key(|h| pos.get(&h.id.0).copied().unwrap_or(usize::MAX));
    let mut per_kind: std::collections::HashMap<MediaKind, usize> = Default::default();
    Ok(hits
        .into_iter()
        .filter(|h| {
            let n = per_kind.entry(h.kind).or_default();
            *n += 1;
            *n <= limit_per_kind
        })
        .collect())
}

/// Each whitespace-separated word becomes a quoted prefix term, so user input can't inject
/// FTS5 syntax.
fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Items with a saved position, most recent first.
pub async fn continue_watching(
    db: &Db,
    filter: &ItemFilter,
    limit: u32,
) -> Result<Vec<ItemSummary>> {
    let mut qb = summary_query(filter);
    qb.push(" AND w.view_offset_ms > 0 AND i.kind IN ('movie', 'episode')");
    qb.push(" ORDER BY w.last_viewed_at DESC LIMIT ").push_bind(i64::from(limit));
    Ok(qb.build_query_as().fetch_all(&db.read).await?)
}

/// The first unwatched episode of every show the user has watched something of.
pub async fn up_next(db: &Db, filter: &ItemFilter, limit: u32) -> Result<Vec<ItemSummary>> {
    let mut qb = summary_query(filter);
    qb.push(
        " AND i.id IN (
            SELECT id FROM (
                SELECT e.id, s.parent_id AS show_id, MAX(COALESCE(ws.last_viewed_at, 0))
                    OVER (PARTITION BY s.parent_id) AS show_last,
                    ROW_NUMBER() OVER (PARTITION BY s.parent_id ORDER BY s.idx, e.idx, e.id) AS rn
                FROM media_items e
                JOIN media_items s ON s.id = e.parent_id
                LEFT JOIN watch_state ws ON ws.item_id = e.id AND ws.user_id = ",
    )
    .push_bind(filter.user_id.0)
    .push(
        " WHERE e.kind = 'episode'
                  AND s.parent_id IN (
                      SELECT s2.parent_id FROM watch_state w2
                      JOIN media_items e2 ON e2.id = w2.item_id
                      JOIN media_items s2 ON s2.id = e2.parent_id
                      WHERE w2.user_id = ",
    )
    .push_bind(filter.user_id.0)
    .push(
        " AND w2.view_count > 0)
                  AND COALESCE(ws.view_count, 0) = 0 AND COALESCE(ws.view_offset_ms, 0) = 0
            ) WHERE rn = 1
        )",
    );
    qb.push(" ORDER BY i.id DESC LIMIT ").push_bind(i64::from(limit));
    Ok(qb.build_query_as().fetch_all(&db.read).await?)
}

/// Newest top-level items of one library.
pub async fn recently_added(
    db: &Db,
    filter: &ItemFilter,
    library: LibraryId,
    limit: u32,
) -> Result<Vec<ItemSummary>> {
    let mut qb = summary_query(filter);
    qb.push(" AND i.library_id = ").push_bind(library.0);
    qb.push(" AND i.parent_id IS NULL AND i.kind != 'extra'");
    qb.push(" ORDER BY i.added_at DESC, i.id DESC LIMIT ").push_bind(i64::from(limit));
    Ok(qb.build_query_as().fetch_all(&db.read).await?)
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    pub id: ItemId,
    pub library_id: LibraryId,
    pub kind: MediaKind,
    pub parent_id: Option<ItemId>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub idx: Option<i64>,
    pub parent_idx: Option<i64>,
    pub originally_available_at: Option<String>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    pub content_rating: Option<String>,
    pub studio: Option<String>,
    pub rating: Option<f64>,
    pub duration_ms: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub needs_review: bool,
    pub added_at: i64,
    pub genres: Vec<String>,
    pub people: Vec<Credit>,
    pub artwork: Vec<ArtworkInfo>,
    pub versions: Vec<Version>,
    pub watch: WatchState,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Credit {
    pub name: String,
    pub role: String,
    pub character: Option<String>,
    pub thumb_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkInfo {
    pub id: ArtworkId,
    pub kind: String,
    /// Content hash once downloaded; pass as `?v=` for immutable caching.
    pub hash: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub blurhash: Option<String>,
    pub palette: Vec<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: VersionId,
    pub edition: Option<String>,
    pub container: Option<String>,
    pub bitrate: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub hdr: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i64>,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub id: PartId,
    pub size: i64,
    pub duration_ms: Option<i64>,
    pub part_index: i64,
    pub missing: bool,
    pub streams: Vec<Stream>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub idx: i64,
    pub kind: StreamKind,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub hdr: Option<String>,
    pub channels: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WatchState {
    pub view_offset_ms: i64,
    pub view_count: i64,
    pub last_viewed_at: Option<i64>,
}

/// Item with versions, people, genres, artwork and the user's watch state. `None` when the
/// item does not exist or is outside the filter.
pub async fn detail(db: &Db, filter: &ItemFilter, id: ItemId) -> Result<Option<ItemDetail>> {
    let Some(i) = sqlx::query!(
        r#"SELECT id AS "id: ItemId", library_id AS "library_id: LibraryId",
                  kind AS "kind: MediaKind", parent_id AS "parent_id: ItemId", title,
                  original_title, year, idx, parent_idx, originally_available_at, summary,
                  tagline, content_rating, studio, rating, duration_ms, tmdb_id, imdb_id,
                  needs_review AS "needs_review: bool", added_at
           FROM media_items WHERE id = ?"#,
        id
    )
    .fetch_optional(&db.read)
    .await?
    else {
        return Ok(None);
    };
    if !filter.allows(i.library_id) || !filter.allows_rating(i.content_rating.as_deref()) {
        return Ok(None);
    }

    let genres = sqlx::query_scalar!(
        "SELECT t.name FROM item_tags it JOIN tags t ON t.id = it.tag_id
         WHERE it.item_id = ? AND t.kind = 'genre' ORDER BY t.name",
        id
    )
    .fetch_all(&db.read)
    .await?;

    let people = sqlx::query_as!(
        Credit,
        "SELECT p.name, ip.role, ip.character, p.thumb_url FROM item_people ip
         JOIN people p ON p.id = ip.person_id WHERE ip.item_id = ?
         ORDER BY ip.role, ip.ord IS NULL, ip.ord, p.name",
        id
    )
    .fetch_all(&db.read)
    .await?;

    let artwork = sqlx::query!(
        r#"SELECT id AS "id: ArtworkId", kind, local_hash, width, height, blurhash, palette,
                  selected AS "selected: bool"
           FROM artwork WHERE item_id = ? ORDER BY kind, selected DESC, id"#,
        id
    )
    .fetch_all(&db.read)
    .await?
    .into_iter()
    .map(|a| ArtworkInfo {
        id: a.id,
        kind: a.kind,
        hash: a.local_hash,
        width: a.width,
        height: a.height,
        blurhash: a.blurhash,
        palette: a.palette.as_deref().map(parse_palette).unwrap_or_default(),
        selected: a.selected,
    })
    .collect();

    let mut versions: Vec<Version> = sqlx::query!(
        r#"SELECT v.id AS "id: VersionId", v.edition, v.container, v.bitrate, v.width, v.height,
                  v.video_codec, v.hdr, v.audio_codec, v.audio_channels
           FROM media_versions v WHERE v.item_id = ?1
              OR v.id IN (SELECT version_id FROM item_versions WHERE item_id = ?1)
           ORDER BY v.id"#,
        id
    )
    .fetch_all(&db.read)
    .await?
    .into_iter()
    .map(|v| Version {
        id: v.id,
        edition: v.edition,
        container: v.container,
        bitrate: v.bitrate,
        width: v.width,
        height: v.height,
        video_codec: v.video_codec,
        hdr: v.hdr,
        audio_codec: v.audio_codec,
        audio_channels: v.audio_channels,
        parts: Vec::new(),
    })
    .collect();

    for version in &mut versions {
        let parts = sqlx::query!(
            r#"SELECT id AS "id: PartId", size, duration_ms, part_index, missing_since
               FROM media_parts WHERE version_id = ? ORDER BY part_index, id"#,
            version.id
        )
        .fetch_all(&db.read)
        .await?;
        for p in parts {
            let streams = sqlx::query_as!(
                Stream,
                r#"SELECT idx, kind AS "kind: StreamKind", codec, language, title,
                          is_default AS "is_default: bool", is_forced AS "is_forced: bool",
                          width, height, hdr, channels
                   FROM streams WHERE part_id = ? ORDER BY idx"#,
                p.id
            )
            .fetch_all(&db.read)
            .await?;
            version.parts.push(Part {
                id: p.id,
                size: p.size,
                duration_ms: p.duration_ms,
                part_index: p.part_index,
                missing: p.missing_since.is_some(),
                streams,
            });
        }
    }

    let watch = sqlx::query_as!(
        WatchState,
        "SELECT view_offset_ms, view_count, last_viewed_at FROM watch_state
         WHERE user_id = ? AND item_id = ?",
        filter.user_id,
        id
    )
    .fetch_optional(&db.read)
    .await?
    .unwrap_or_default();

    Ok(Some(ItemDetail {
        id: i.id,
        library_id: i.library_id,
        kind: i.kind,
        parent_id: i.parent_id,
        title: i.title,
        original_title: i.original_title,
        year: i.year,
        idx: i.idx,
        parent_idx: i.parent_idx,
        originally_available_at: i.originally_available_at,
        summary: i.summary,
        tagline: i.tagline,
        content_rating: i.content_rating,
        studio: i.studio,
        rating: i.rating,
        duration_ms: i.duration_ms,
        tmdb_id: i.tmdb_id,
        imdb_id: i.imdb_id,
        needs_review: i.needs_review,
        added_at: i.added_at,
        genres,
        people,
        artwork,
        versions,
        watch,
    }))
}

fn parse_palette(raw: &str) -> Vec<String> {
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(raw) {
        return arr;
    }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw)
        && let Some(obj) = val.as_object()
    {
        let mut out = Vec::with_capacity(2);
        if let Some(d) = obj.get("dominant").and_then(|v| v.as_str()) {
            out.push(d.to_owned());
        }
        if let Some(v) = obj.get("vibrant").and_then(|v| v.as_str()) {
            out.push(v.to_owned());
        }
        return out;
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_quotes_terms() {
        assert_eq!(fts_query("the  mat\"rix").as_deref(), Some("\"the\"* \"matrix\"*"));
        assert_eq!(fts_query("  "), None);
        assert_eq!(fts_query("OR NEAR(").as_deref(), Some("\"OR\"* \"NEAR(\"*"));
    }

    #[test]
    fn parses_object_and_array_palettes() {
        assert_eq!(
            parse_palette(r##"{"dominant":"#141414","vibrant":"#dc1e1e"}"##),
            vec!["#141414".to_string(), "#dc1e1e".to_string()]
        );
        assert_eq!(
            parse_palette(r##"["#112233","#445566"]"##),
            vec!["#112233".to_string(), "#445566".to_string()]
        );
    }
}
