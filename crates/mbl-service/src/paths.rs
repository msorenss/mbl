//! Filesystem path rules for library roots and the admin folder browser.

use std::path::{Path, PathBuf};

use mbl_core::{Error, Result};

/// Validate a library root and return its canonical, UTF-8 form.
pub(crate) fn canonical_root(raw: &str) -> Result<String> {
    let raw = raw.trim();
    let path = Path::new(raw);
    if raw.is_empty() || !path.is_absolute() {
        return Err(Error::Validation(format!("library path must be absolute: {raw:?}")));
    }
    let meta = std::fs::metadata(path)
        .map_err(|_| Error::Validation(format!("path does not exist: {raw}")))?;
    if !meta.is_dir() {
        return Err(Error::Validation(format!("path is not a directory: {raw}")));
    }
    std::fs::read_dir(path)
        .map_err(|_| Error::Validation(format!("path is not readable: {raw}")))?;
    let canonical = dunce::canonicalize(path).map_err(Error::from)?;
    utf8(canonical)
}

fn utf8(path: PathBuf) -> Result<String> {
    path.into_os_string()
        .into_string()
        .map_err(|p| Error::Validation(format!("path is not valid UTF-8: {}", p.to_string_lossy())))
}

/// True if one path equals or contains the other (component-wise).
pub(crate) fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    Path::new(&a).starts_with(&b) || Path::new(&b).starts_with(&a)
}

fn normalize(p: &str) -> String {
    if cfg!(windows) { p.to_lowercase() } else { p.to_owned() }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DirListing {
    /// `None` for the virtual top level (filesystem root list).
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<DirEntry>,
}

/// List sub-directories of `path`, or the filesystem roots when `path` is `None`.
pub(crate) fn list_dirs(path: Option<&str>) -> Result<DirListing> {
    let Some(raw) = path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(DirListing { path: None, parent: None, entries: roots() });
    };
    let dir = canonical_root(raw)?;
    let mut entries: Vec<DirEntry> = std::fs::read_dir(&dir)?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let path = e.path().into_os_string().into_string().ok()?;
            (!is_hidden(&name)).then_some(DirEntry { name, path })
        })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    let parent = Path::new(&dir).parent().and_then(|p| p.to_str()).map(str::to_owned);
    Ok(DirListing { path: Some(dir), parent, entries })
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "$RECYCLE.BIN" | "System Volume Information" | "lost+found" | "@eaDir")
}

#[cfg(windows)]
fn roots() -> Vec<DirEntry> {
    ('A'..='Z')
        .map(|d| format!("{d}:\\"))
        .filter(|p| Path::new(p).exists())
        .map(|p| DirEntry { name: p.clone(), path: p })
        .collect()
}

#[cfg(not(windows))]
fn roots() -> Vec<DirEntry> {
    vec![DirEntry { name: "/".into(), path: "/".into() }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_is_component_wise() {
        assert!(overlaps("/media/movies", "/media/movies"));
        assert!(overlaps("/media", "/media/movies"));
        assert!(overlaps("/media/movies/hd", "/media/movies"));
        assert!(!overlaps("/media/movies", "/media/movies2"));
        assert!(!overlaps("/media/tv", "/media/movies"));
    }

    #[test]
    fn canonical_root_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        assert!(canonical_root(root).is_ok());
        assert!(canonical_root("relative/path").is_err());
        assert!(canonical_root(&format!("{root}/missing")).is_err());

        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(canonical_root(file.to_str().unwrap()).is_err());
    }

    #[test]
    fn lists_visible_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["Movies", "tv", ".hidden", "lost+found"] {
            std::fs::create_dir(dir.path().join(d)).unwrap();
        }
        std::fs::write(dir.path().join("file.mkv"), "x").unwrap();
        let listing = list_dirs(dir.path().to_str()).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Movies", "tv"]);
        assert!(listing.parent.is_some());
        assert!(!list_dirs(None).unwrap().entries.is_empty());
    }
}
