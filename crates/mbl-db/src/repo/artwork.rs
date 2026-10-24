use mbl_core::{ArtworkId, ItemId, LibraryId, Result};

use crate::Db;

#[derive(Debug, Clone, PartialEq)]
pub struct Artwork {
    pub id: ArtworkId,
    pub item_id: ItemId,
    pub library_id: LibraryId,
    pub kind: String,
    pub source_url: Option<String>,
    pub local_hash: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub blurhash: Option<String>,
    pub palette: Option<String>,
    pub selected: bool,
}

pub async fn get(db: &Db, id: ArtworkId) -> Result<Option<Artwork>> {
    Ok(sqlx::query_as!(
        Artwork,
        r#"SELECT a.id AS "id: ArtworkId", a.item_id AS "item_id: ItemId",
                  i.library_id AS "library_id: LibraryId", a.kind, a.source_url, a.local_hash,
                  a.width, a.height, a.blurhash, a.palette, a.selected AS "selected: bool"
           FROM artwork a JOIN media_items i ON i.id = a.item_id WHERE a.id = ?"#,
        id
    )
    .fetch_optional(&db.read)
    .await?)
}

/// Artwork of an item and all its descendants that has not been downloaded yet.
pub async fn pending_for_tree(db: &Db, root: ItemId) -> Result<Vec<ArtworkId>> {
    Ok(sqlx::query_scalar!(
        r#"WITH RECURSIVE tree(id) AS (
               SELECT ?1 UNION ALL SELECT m.id FROM media_items m JOIN tree t ON m.parent_id = t.id)
           SELECT a.id AS "id: ArtworkId" FROM artwork a
           WHERE a.item_id IN (SELECT id FROM tree) AND a.local_hash IS NULL
             AND a.source_url IS NOT NULL
           ORDER BY a.selected DESC, a.id"#,
        root
    )
    .fetch_all(&db.read)
    .await?)
}

pub async fn set_local(
    db: &Db,
    id: ArtworkId,
    hash: &str,
    width: i64,
    height: i64,
    blurhash: &str,
    palette: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE artwork SET local_hash = ?, width = ?, height = ?, blurhash = ?, palette = ? WHERE id = ?",
        hash,
        width,
        height,
        blurhash,
        palette,
        id
    )
    .execute(&db.write)
    .await?;
    Ok(())
}
