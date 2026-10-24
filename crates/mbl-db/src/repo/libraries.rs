use mbl_core::{LibraryId, LibraryKind, Result};
use serde::Serialize;

use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Library {
    pub id: LibraryId,
    pub name: String,
    pub kind: LibraryKind,
    pub created_at: i64,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LibrariesRepo {
    db: Db,
}

impl LibrariesRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn insert(
        &self,
        name: &str,
        kind: LibraryKind,
        paths: &[String],
        now: i64,
    ) -> Result<LibraryId> {
        let mut tx = self.db.write.begin().await?;
        let id = sqlx::query_scalar!(
            r#"INSERT INTO libraries (name, kind, created_at) VALUES (?, ?, ?)
               RETURNING id AS "id!: LibraryId""#,
            name,
            kind,
            now
        )
        .fetch_one(&mut *tx)
        .await?;
        for path in paths {
            sqlx::query!(
                "INSERT INTO library_paths (library_id, path) VALUES (?, ?)",
                id,
                path
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::map_unique(e, "path is already used by a library"))?;
        }
        tx.commit().await?;
        Ok(id)
    }

    pub async fn list(&self) -> Result<Vec<Library>> {
        let rows = sqlx::query!(
            r#"SELECT id AS "id: LibraryId", name, kind AS "kind: LibraryKind", created_at
               FROM libraries ORDER BY name COLLATE NOCASE, id"#
        )
        .fetch_all(&self.db.read)
        .await?;
        let paths = self.all_paths().await?;
        Ok(rows
            .into_iter()
            .map(|r| Library {
                paths: paths
                    .iter()
                    .filter(|(lib, _)| *lib == r.id)
                    .map(|(_, p)| p.clone())
                    .collect(),
                id: r.id,
                name: r.name,
                kind: r.kind,
                created_at: r.created_at,
            })
            .collect())
    }

    pub async fn get(&self, id: LibraryId) -> Result<Option<Library>> {
        let Some(r) = sqlx::query!(
            r#"SELECT id AS "id: LibraryId", name, kind AS "kind: LibraryKind", created_at
               FROM libraries WHERE id = ?"#,
            id
        )
        .fetch_optional(&self.db.read)
        .await?
        else {
            return Ok(None);
        };
        let paths = sqlx::query_scalar!(
            "SELECT path FROM library_paths WHERE library_id = ? ORDER BY path",
            id
        )
        .fetch_all(&self.db.read)
        .await?;
        Ok(Some(Library {
            id: r.id,
            name: r.name,
            kind: r.kind,
            created_at: r.created_at,
            paths,
        }))
    }

    /// Every configured root as `(library, path)`.
    pub async fn all_paths(&self) -> Result<Vec<(LibraryId, String)>> {
        let rows = sqlx::query!(
            r#"SELECT library_id AS "library_id: LibraryId", path FROM library_paths ORDER BY path"#
        )
        .fetch_all(&self.db.read)
        .await?;
        Ok(rows.into_iter().map(|r| (r.library_id, r.path)).collect())
    }

    pub async fn update_name(&self, id: LibraryId, name: &str) -> Result<bool> {
        let res = sqlx::query!("UPDATE libraries SET name = ? WHERE id = ?", name, id)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Replace the set of roots. Roots that remain keep their row (and `last_scan_at`).
    pub async fn replace_paths(&self, id: LibraryId, paths: &[String]) -> Result<()> {
        let mut tx = self.db.write.begin().await?;
        let existing =
            sqlx::query_scalar!("SELECT path FROM library_paths WHERE library_id = ?", id)
                .fetch_all(&mut *tx)
                .await?;
        for old in existing.iter().filter(|p| !paths.contains(p)) {
            sqlx::query!(
                "DELETE FROM library_paths WHERE library_id = ? AND path = ?",
                id,
                old
            )
            .execute(&mut *tx)
            .await?;
        }
        for new in paths.iter().filter(|p| !existing.contains(p)) {
            sqlx::query!(
                "INSERT INTO library_paths (library_id, path) VALUES (?, ?)",
                id,
                new
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| crate::map_unique(e, "path is already used by a library"))?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete(&self, id: LibraryId) -> Result<bool> {
        let res = sqlx::query!("DELETE FROM libraries WHERE id = ?", id)
            .execute(&self.db.write)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}
