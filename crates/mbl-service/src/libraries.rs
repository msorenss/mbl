use mbl_core::{Actor, Error, LibraryId, LibraryKind, Result, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::libraries::{LibrariesRepo, Library};

use crate::blocking;
use crate::paths::{self, DirListing};

#[derive(Debug, Clone)]
pub struct CreateLibrary {
    pub name: String,
    pub kind: LibraryKind,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateLibrary {
    pub name: Option<String>,
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct LibraryService {
    repo: LibrariesRepo,
}

impl LibraryService {
    pub fn new(db: Db) -> Self {
        Self {
            repo: LibrariesRepo::new(db),
        }
    }

    /// Libraries visible to the actor.
    pub async fn list(&self, actor: &Actor) -> Result<Vec<Library>> {
        let mut libs = self.repo.list().await?;
        libs.retain(|l| actor.can_access_library(l.id));
        Ok(libs)
    }

    pub async fn get(&self, actor: &Actor, id: LibraryId) -> Result<Library> {
        if !actor.can_access_library(id) {
            return Err(Error::NotFound);
        }
        self.repo.get(id).await?.ok_or(Error::NotFound)
    }

    pub async fn create(&self, actor: &Actor, input: CreateLibrary) -> Result<Library> {
        actor.require_admin()?;
        let name = library_name(&input.name)?;
        let roots = self.validate_roots(input.paths, None).await?;
        let id = self
            .repo
            .insert(&name, input.kind, &roots, now_ms())
            .await?;
        tracing::info!(library_id = %id, kind = ?input.kind, "library created");
        self.repo.get(id).await?.ok_or(Error::NotFound)
    }

    pub async fn update(
        &self,
        actor: &Actor,
        id: LibraryId,
        patch: UpdateLibrary,
    ) -> Result<Library> {
        actor.require_admin()?;
        self.repo.get(id).await?.ok_or(Error::NotFound)?;
        let name = patch.name.as_deref().map(library_name).transpose()?;
        let roots = match patch.paths {
            Some(p) => Some(self.validate_roots(p, Some(id)).await?),
            None => None,
        };
        if let Some(name) = name {
            self.repo.update_name(id, &name).await?;
        }
        if let Some(roots) = roots {
            self.repo.replace_paths(id, &roots).await?;
        }
        self.repo.get(id).await?.ok_or(Error::NotFound)
    }

    pub async fn delete(&self, actor: &Actor, id: LibraryId) -> Result<()> {
        actor.require_admin()?;
        if !self.repo.delete(id).await? {
            return Err(Error::NotFound);
        }
        tracing::info!(library_id = %id, "library deleted");
        Ok(())
    }

    /// Admin folder browser for picking library roots.
    pub async fn browse(&self, actor: &Actor, path: Option<String>) -> Result<DirListing> {
        actor.require_admin()?;
        blocking(move || paths::list_dirs(path.as_deref())).await
    }

    /// Canonicalize roots and reject duplicates or nesting, within the input and against other
    /// libraries (`except` = the library being updated).
    async fn validate_roots(
        &self,
        raw: Vec<String>,
        except: Option<LibraryId>,
    ) -> Result<Vec<String>> {
        if raw.is_empty() {
            return Err(Error::Validation(
                "a library needs at least one path".into(),
            ));
        }
        let roots = blocking(move || {
            raw.iter()
                .map(|p| paths::canonical_root(p))
                .collect::<Result<Vec<_>>>()
        })
        .await?;
        for (i, a) in roots.iter().enumerate() {
            if roots[i + 1..].iter().any(|b| paths::overlaps(a, b)) {
                return Err(Error::Validation(format!("paths overlap: {a}")));
            }
        }
        for (lib, existing) in self.repo.all_paths().await? {
            if Some(lib) == except {
                continue;
            }
            if let Some(root) = roots.iter().find(|r| paths::overlaps(r, &existing)) {
                return Err(Error::Conflict(format!(
                    "{root} overlaps {existing} (library {lib})"
                )));
            }
        }
        Ok(roots)
    }
}

fn library_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(Error::Validation(
            "library name must be 1-100 printable characters".into(),
        ));
    }
    Ok(name.to_owned())
}
