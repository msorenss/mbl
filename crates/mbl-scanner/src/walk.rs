//! Filesystem walking: media extensions, ignore files, system folders, samples.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use mbl_core::LibraryKind;

pub const VIDEO_EXTS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "wmv", "ts", "m2ts", "mts", "webm", "mpg", "mpeg", "vob",
    "flv", "ogv", "3gp", "divx",
];
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "wma", "aiff", "aif", "ape", "wv",
    "dsf", "dff", "mka",
];

/// Folders created by NAS software, OSes and recycle bins.
const SKIP_DIRS: &[&str] =
    &["@eaDir", "#recycle", "$RECYCLE.BIN", "System Volume Information", "lost+found"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundFile {
    pub path: String,
    pub root: String,
    pub size: i64,
    pub mtime: i64,
}

impl FoundFile {
    /// Path relative to its library root, used for parsing.
    pub fn relative(&self) -> &str {
        self.path.strip_prefix(&self.root).unwrap_or(&self.path).trim_start_matches(['/', '\\'])
    }
}

pub fn extensions(kind: LibraryKind) -> &'static [&'static str] {
    match kind {
        LibraryKind::Movie | LibraryKind::Show | LibraryKind::HomeVideo => VIDEO_EXTS,
        LibraryKind::Music => AUDIO_EXTS,
        // Photo libraries arrive in M7.5.
        LibraryKind::Photo => &[],
    }
}

fn has_ext(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Recursively list media files under `root`. Honors `.mblignore` / `.plexignore` (gitignore
/// syntax), skips hidden and system folders and release samples. Blocking: run off the executor.
#[cfg(test)]
pub fn walk(root: &str, kind: LibraryKind) -> Vec<FoundFile> {
    walk_from(root, root, kind)
}

/// Recursive listing of media files under `start`, a folder inside `root` (`.mblignore` /
/// `.plexignore`, hidden/system folders and samples are skipped). A missing `start` yields nothing.
pub fn walk_from(root: &str, start: &str, kind: LibraryKind) -> Vec<FoundFile> {
    let exts = extensions(kind);
    if exts.is_empty() || !Path::new(start).is_dir() {
        return Vec::new();
    }
    let walker = ignore::WalkBuilder::new(start)
        .standard_filters(false)
        .hidden(true)
        .add_custom_ignore_filename(".mblignore")
        .add_custom_ignore_filename(".plexignore")
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_some_and(|t| t.is_dir())
                && (SKIP_DIRS.iter().any(|d| name.eq_ignore_ascii_case(d))
                    || name.starts_with(".Trash-")))
        })
        .build();

    let mut files = Vec::new();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "skipping unreadable path");
                continue;
            }
        };
        let path = entry.path();
        if !entry.file_type().is_some_and(|t| t.is_file()) || !has_ext(path, exts) {
            continue;
        }
        let Some(path_str) = path.to_str() else {
            tracing::warn!(path = %path.display(), "skipping non-UTF-8 path");
            continue;
        };
        if mbl_parser::is_sample(path_str) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        files.push(FoundFile {
            path: path_str.to_owned(),
            root: root.to_owned(),
            size: i64::try_from(meta.len()).unwrap_or(i64::MAX),
            mtime: mtime_ms(&meta),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

pub fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// A root is unavailable when it cannot be read, or is empty while we know files under it
/// (typical for an unmounted USB drive or network share).
pub fn root_available(root: &str, has_known_files: bool) -> bool {
    match std::fs::read_dir(PathBuf::from(root)) {
        Ok(mut entries) => entries.next().is_some() || !has_known_files,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_media_and_honors_ignores() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mk = |rel: &str| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        };
        mk("Movie (2020)/Movie (2020).mkv");
        mk("Movie (2020)/Movie (2020).nfo");
        mk("Movie (2020)/movie-sample.mkv");
        mk(".hidden/Hidden.mkv");
        mk("@eaDir/Thumb.mkv");
        mk("Skipped/Skipped.mkv");
        mk("Partly/Keep.mkv");
        mk("Partly/Drop.mkv");
        std::fs::write(root.join("Skipped/.mblignore"), "*\n").unwrap();
        std::fs::write(root.join("Partly/.plexignore"), "Drop.mkv\n").unwrap();

        let root_str = root.to_str().unwrap();
        let found: Vec<_> =
            walk(root_str, LibraryKind::Movie).iter().map(|f| f.relative().to_owned()).collect();
        assert_eq!(found, ["Movie (2020)/Movie (2020).mkv", "Partly/Keep.mkv"]);
        assert!(walk(root_str, LibraryKind::Music).is_empty());
    }

    #[test]
    fn availability() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        assert!(root_available(root, false));
        assert!(!root_available(root, true), "empty root with known files looks unmounted");
        assert!(!root_available(&format!("{root}/missing"), false));
    }
}
