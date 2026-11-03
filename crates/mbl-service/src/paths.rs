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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub is_file: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
pub struct DirListing {
    /// `None` for the virtual top level (filesystem root list).
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<DirEntry>,
}

/// List sub-directories (and optionally files matching `ext`, e.g. `"hef"`) of `path`,
/// or the filesystem roots when `path` is `None`.
pub(crate) fn list_dirs(path: Option<&str>, ext: Option<&str>) -> Result<DirListing> {
    let Some(raw) = path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(DirListing { path: None, parent: None, entries: roots() });
    };
    let dir = canonical_root(raw)?;
    let ext_filter = ext
        .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty());

    let mut entries: Vec<DirEntry> = std::fs::read_dir(&dir)?
        .filter_map(std::result::Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let is_dir = meta.is_dir();
            let is_file = meta.is_file();
            if !is_dir && !is_file {
                return None;
            }
            let name = e.file_name().into_string().ok()?;
            if is_hidden(&name) {
                return None;
            }
            if is_file {
                let want_ext = ext_filter.as_deref()?;
                let file_ext = Path::new(&name).extension().and_then(|x| x.to_str()).unwrap_or("");
                if !file_ext.eq_ignore_ascii_case(want_ext) {
                    return None;
                }
            }
            let path = e.path().into_os_string().into_string().ok()?;
            Some(DirEntry { name, path, is_file, size_bytes: is_file.then_some(meta.len()) })
        })
        .collect();
    // Sort directories first, then files alphabetically
    entries.sort_by_key(|e| (e.is_file, e.name.to_lowercase()));
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
        .map(|p| DirEntry { name: p.clone(), path: p, is_file: false, size_bytes: None })
        .collect()
}

#[cfg(not(windows))]
fn roots() -> Vec<DirEntry> {
    vec![DirEntry { name: "/".into(), path: "/".into(), is_file: false, size_bytes: None }]
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
    fn lists_visible_subdirectories_and_optional_file_extensions() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["Movies", "tv", ".hidden", "lost+found"] {
            std::fs::create_dir(dir.path().join(d)).unwrap();
        }
        std::fs::write(dir.path().join("file.mkv"), "x").unwrap();
        std::fs::write(dir.path().join("yolov11s.hef"), "hefdata").unwrap();

        let listing = list_dirs(dir.path().to_str(), None).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Movies", "tv"]);
        assert!(listing.parent.is_some());
        assert!(!list_dirs(None, None).unwrap().entries.is_empty());

        let listing_hef = list_dirs(dir.path().to_str(), Some("hef")).unwrap();
        let names_hef: Vec<_> = listing_hef.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names_hef, ["Movies", "tv", "yolov11s.hef"]);
        assert!(listing_hef.entries[2].is_file);
        assert_eq!(listing_hef.entries[2].size_bytes, Some(7));
    }
}
