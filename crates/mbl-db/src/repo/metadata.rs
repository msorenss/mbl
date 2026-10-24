//! Reading and writing item metadata (matching results, people, tags, artwork URLs).

use mbl_core::{ItemId, LibraryId, MediaKind, Result};
use sqlx::SqliteConnection;

use crate::Db;

#[derive(Debug, Clone)]
pub struct MatchTarget {
    pub id: ItemId,
    pub library_id: LibraryId,
    pub kind: MediaKind,
    pub parent_id: Option<ItemId>,
    pub title: String,
    pub year: Option<i64>,
    pub idx: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub locked_fields: String,
}

/// Field values to write; `None` leaves the current value untouched.
#[derive(Debug, Clone, Default)]
pub struct MetadataUpdate {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    pub year: Option<i64>,
    pub originally_available_at: Option<String>,
    pub content_rating: Option<String>,
    pub studio: Option<String>,
    pub rating: Option<f64>,
    pub tmdb_id: Option<i64>,
    pub tvdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub match_confidence: Option<f64>,
    pub needs_review: Option<bool>,
}

impl MetadataUpdate {
    /// Drop fields the user locked (`locked_fields` is a JSON array of field names).
    pub fn without_locked(mut self, locked_json: &str) -> Self {
        let locked: Vec<String> = serde_json::from_str(locked_json).unwrap_or_default();
        let is = |f: &str| locked.iter().any(|l| l == f);
        if is("title") {
            self.title = None;
        }
        if is("summary") {
            self.summary = None;
        }
        if is("tagline") {
            self.tagline = None;
        }
        if is("year") {
            self.year = None;
        }
        if is("originally_available_at") {
            self.originally_available_at = None;
        }
        if is("content_rating") {
            self.content_rating = None;
        }
        if is("studio") {
            self.studio = None;
        }
        self
    }
}

#[derive(Debug, Clone)]
pub struct PersonCredit {
    pub name: String,
    pub tmdb_id: Option<i64>,
    pub thumb_url: Option<String>,
    pub role: String,
    pub character: Option<String>,
}

pub async fn match_target(db: &Db, id: ItemId) -> Result<Option<MatchTarget>> {
    Ok(sqlx::query_as!(
        MatchTarget,
        r#"SELECT id AS "id: ItemId", library_id AS "library_id: LibraryId", kind AS "kind: MediaKind",
                  parent_id AS "parent_id: ItemId", title, year, idx, tmdb_id, tvdb_id, imdb_id,
                  locked_fields
           FROM media_items WHERE id = ?"#,
        id
    )
    .fetch_optional(&db.read)
    .await?)
}

pub async fn children(db: &Db, parent: ItemId) -> Result<Vec<MatchTarget>> {
    Ok(sqlx::query_as!(
        MatchTarget,
        r#"SELECT id AS "id: ItemId", library_id AS "library_id: LibraryId", kind AS "kind: MediaKind",
                  parent_id AS "parent_id: ItemId", title, year, idx, tmdb_id, tvdb_id, imdb_id,
                  locked_fields
           FROM media_items WHERE parent_id = ? ORDER BY idx"#,
        parent
    )
    .fetch_all(&db.read)
    .await?)
}

/// Top-level items (movies, shows, artists) of a library.
pub async fn top_level_items(db: &Db, library: LibraryId) -> Result<Vec<ItemId>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT id AS "id: ItemId" FROM media_items
           WHERE library_id = ? AND parent_id IS NULL ORDER BY id"#,
        library
    )
    .fetch_all(&db.read)
    .await?)
}

/// Path of the first media file under an item (or its first descendant with a file).
pub async fn first_file(db: &Db, id: ItemId) -> Result<Option<String>> {
    Ok(sqlx::query_scalar!(
        r#"WITH RECURSIVE tree(id) AS (
               SELECT ?1 UNION ALL SELECT m.id FROM media_items m JOIN tree t ON m.parent_id = t.id)
           SELECT p.path FROM media_parts p
           JOIN item_versions iv ON iv.version_id = p.version_id
           WHERE iv.item_id IN (SELECT id FROM tree)
           ORDER BY p.path LIMIT 1"#,
        id
    )
    .fetch_optional(&db.read)
    .await?)
}

pub async fn apply(
    conn: &mut SqliteConnection,
    id: ItemId,
    u: &MetadataUpdate,
    now: i64,
) -> Result<()> {
    let sort = u.title.as_deref().map(sort_title);
    sqlx::query!(
        r#"UPDATE media_items SET
             title = COALESCE(?, title),
             sort_title = COALESCE(?, sort_title),
             original_title = COALESCE(?, original_title),
             summary = COALESCE(?, summary),
             tagline = COALESCE(?, tagline),
             year = COALESCE(?, year),
             originally_available_at = COALESCE(?, originally_available_at),
             content_rating = COALESCE(?, content_rating),
             studio = COALESCE(?, studio),
             rating = COALESCE(?, rating),
             tmdb_id = COALESCE(?, tmdb_id),
             tvdb_id = COALESCE(?, tvdb_id),
             imdb_id = COALESCE(?, imdb_id),
             match_confidence = COALESCE(?, match_confidence),
             needs_review = COALESCE(?, needs_review),
             metadata_refreshed_at = ?,
             updated_at = ?
           WHERE id = ?"#,
        u.title,
        sort,
        u.original_title,
        u.summary,
        u.tagline,
        u.year,
        u.originally_available_at,
        u.content_rating,
        u.studio,
        u.rating,
        u.tmdb_id,
        u.tvdb_id,
        u.imdb_id,
        u.match_confidence,
        u.needs_review,
        now,
        now,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn sort_title(title: &str) -> String {
    let lower = title.trim().to_lowercase();
    ["the ", "a ", "an "]
        .iter()
        .find_map(|a| lower.strip_prefix(a).filter(|r| !r.is_empty()).map(str::to_owned))
        .unwrap_or(lower)
}

pub async fn replace_people(
    conn: &mut SqliteConnection,
    item: ItemId,
    people: &[PersonCredit],
) -> Result<()> {
    sqlx::query!("DELETE FROM item_people WHERE item_id = ?", item).execute(&mut *conn).await?;
    for (ord, p) in people.iter().enumerate() {
        let existing = match p.tmdb_id {
            Some(t) => {
                sqlx::query_scalar!(r#"SELECT id AS "id!" FROM people WHERE tmdb_id = ?"#, t)
                    .fetch_optional(&mut *conn)
                    .await?
            }
            None => {
                sqlx::query_scalar!(
                    r#"SELECT id AS "id!" FROM people WHERE tmdb_id IS NULL AND name = ?"#,
                    p.name
                )
                .fetch_optional(&mut *conn)
                .await?
            }
        };
        let person_id = match existing {
            Some(id) => {
                sqlx::query!(
                    "UPDATE people SET name = ?, thumb_url = COALESCE(?, thumb_url) WHERE id = ?",
                    p.name,
                    p.thumb_url,
                    id
                )
                .execute(&mut *conn)
                .await?;
                id
            }
            None => sqlx::query_scalar!(
                r#"INSERT INTO people (name, tmdb_id, thumb_url) VALUES (?, ?, ?) RETURNING id AS "id!""#,
                p.name,
                p.tmdb_id,
                p.thumb_url
            )
            .fetch_one(&mut *conn)
            .await?,
        };
        let ord = i64::try_from(ord).unwrap_or(i64::MAX);
        sqlx::query!(
            "INSERT OR IGNORE INTO item_people (item_id, person_id, role, character, ord) VALUES (?, ?, ?, ?, ?)",
            item,
            person_id,
            p.role,
            p.character,
            ord
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn replace_tags(
    conn: &mut SqliteConnection,
    item: ItemId,
    kind: &str,
    names: &[String],
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM item_tags WHERE item_id = ? AND tag_id IN (SELECT id FROM tags WHERE kind = ?)",
        item,
        kind
    )
    .execute(&mut *conn)
    .await?;
    for name in names {
        sqlx::query!("INSERT OR IGNORE INTO tags (kind, name) VALUES (?, ?)", kind, name)
            .execute(&mut *conn)
            .await?;
        sqlx::query!(
            "INSERT OR IGNORE INTO item_tags (item_id, tag_id) SELECT ?, id FROM tags WHERE kind = ? AND name = ?",
            item,
            kind,
            name
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Replace remote artwork of one kind (local files and downloads are handled in M1.8).
/// The first URL becomes the selected one.
pub async fn replace_artwork_urls(
    conn: &mut SqliteConnection,
    item: ItemId,
    kind: &str,
    urls: &[String],
) -> Result<()> {
    sqlx::query!("DELETE FROM artwork WHERE item_id = ? AND kind = ?", item, kind)
        .execute(&mut *conn)
        .await?;
    for (i, url) in urls.iter().enumerate() {
        let selected = i == 0;
        sqlx::query!(
            "INSERT INTO artwork (item_id, kind, source_url, selected) VALUES (?, ?, ?, ?)",
            item,
            kind,
            url,
            selected
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
