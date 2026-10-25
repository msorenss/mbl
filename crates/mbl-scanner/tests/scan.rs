#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use camino::Utf8PathBuf;
use mbl_core::events::EventBus;
use mbl_core::{LibraryId, LibraryKind, MediaKind, time::now_ms};
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::media;
use mbl_db::{Db, DbOptions};
use mbl_probe::FfTools;
use mbl_scanner::Scanner;
use tokio_util::sync::CancellationToken;

fn media_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/out")
}

/// DB + scanner, or `None` when FFmpeg or the generated test library is missing.
async fn setup() -> Option<(Db, Scanner, tempfile::TempDir)> {
    if !media_root().join("manifest.json").exists() {
        eprintln!("skipping: run `cargo xtask gen-media --quick` first");
        return None;
    }
    let tools = FfTools::locate("", "").await.ok()?;
    let dir = tempfile::tempdir().unwrap();
    let db_path = Utf8PathBuf::from_path_buf(dir.path().join("mbl.db")).unwrap();
    let db = Db::connect(&db_path, &DbOptions::default()).await.unwrap();
    let scanner = Scanner::new(db.clone(), Some(tools), EventBus::default(), None, 4);
    Some((db, scanner, dir))
}

async fn library(db: &Db, kind: LibraryKind, root: &Path) -> LibraryId {
    let root = dunce_like(root);
    LibrariesRepo::new(db.clone()).insert("Test", kind, &[root], now_ms()).await.unwrap()
}

fn dunce_like(p: &Path) -> String {
    std::fs::canonicalize(p).unwrap().to_str().unwrap().to_owned()
}

async fn count(db: &Db, lib: LibraryId, kind: MediaKind) -> i64 {
    media::count_items(db, lib, kind).await.unwrap()
}

fn manifest_count(kind: &str, prefix: &str) -> i64 {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(media_root().join("manifest.json")).unwrap())
            .unwrap();
    manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == kind && f["path"].as_str().unwrap().starts_with(prefix))
        .count() as i64
}

#[tokio::test]
async fn scans_generated_library_and_rescans_without_probing() {
    let Some((db, scanner, _dir)) = setup().await else { return };
    let cancel = CancellationToken::new();

    let movies = library(&db, LibraryKind::Movie, &media_root().join("Movies")).await;
    let stats = scanner.scan(movies, &cancel, |_| {}).await.unwrap();
    assert_eq!(stats.failed, 0, "{stats:?}");
    assert_eq!(count(&db, movies, MediaKind::Movie).await, manifest_count("movie", "Movies/"));

    let tv = library(&db, LibraryKind::Show, &media_root().join("TV")).await;
    let stats = scanner.scan(tv, &cancel, |_| {}).await.unwrap();
    assert_eq!(stats.failed, 0, "{stats:?}");
    assert_eq!(count(&db, tv, MediaKind::Show).await, 2);
    assert_eq!(count(&db, tv, MediaKind::Season).await, 2);
    // S01E01, S01E02, S01E03-E04 (two episodes, one file) and the interlaced S01E01.
    assert_eq!(count(&db, tv, MediaKind::Episode).await, 5);
    let shared: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM item_versions iv JOIN media_items i ON i.id = iv.item_id
         WHERE i.library_id = ? GROUP BY iv.version_id ORDER BY COUNT(*) DESC LIMIT 1",
    )
    .bind(tv)
    .fetch_one(&db.read)
    .await
    .unwrap();
    assert_eq!(shared, 2, "multi-episode file shares one version");

    let music = library(&db, LibraryKind::Music, &media_root().join("Music")).await;
    let stats = scanner.scan(music, &cancel, |_| {}).await.unwrap();
    assert_eq!(stats.failed, 0, "{stats:?}");
    assert_eq!(count(&db, music, MediaKind::Artist).await, 1);
    assert_eq!(count(&db, music, MediaKind::Album).await, 1);
    assert_eq!(count(&db, music, MediaKind::Track).await, 3);

    // Streams were stored.
    let streams: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM streams").fetch_one(&db.read).await.unwrap();
    assert!(streams > 20);

    let again = scanner.scan(movies, &cancel, |_| {}).await.unwrap();
    assert_eq!((again.probed, again.added, again.missing), (0, 0, 0), "{again:?}");
    assert_eq!(again.unchanged, again.files_seen);
}

fn copy_movie(dest_root: &Path, rel: &str) -> PathBuf {
    let src = media_root().join("Movies/Big Test (2020)/Big Test (2020).mp4");
    let dest = dest_root.join(rel);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::copy(src, &dest).unwrap();
    dest
}

#[tokio::test]
async fn rename_keeps_item_and_missing_files_are_flagged() {
    let Some((db, scanner, dir)) = setup().await else { return };
    let cancel = CancellationToken::new();
    let root = dir.path().join("lib");
    let original = copy_movie(&root, "Big Test (2020)/Big Test (2020).mp4");
    let lib = library(&db, LibraryKind::Movie, &root).await;

    let first = scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    assert_eq!((first.added, first.new_items), (1, 1));
    let parts = media::parts_for_library(&db, lib).await.unwrap();
    let part_id = parts[0].id;

    let renamed = root.join("Big Test (2020)/Big Test (2020) - copy.mp4");
    std::fs::rename(&original, &renamed).unwrap();
    let second = scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    assert_eq!((second.moved, second.probed, second.missing), (1, 0, 0), "{second:?}");
    let parts = media::parts_for_library(&db, lib).await.unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].id, part_id, "part (and its item) survives a rename");
    assert!(parts[0].path.ends_with("Big Test (2020) - copy.mp4"));

    // Delete one of two files: it is flagged missing, not deleted.
    copy_movie(&root, "Other (2021)/Other (2021).mp4");
    scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    std::fs::remove_file(&renamed).unwrap();
    let third = scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    assert_eq!(third.missing, 1, "{third:?}");
    let parts = media::parts_for_library(&db, lib).await.unwrap();
    assert_eq!(parts.iter().filter(|p| p.missing_since.is_some()).count(), 1);

    // Restore it: missing flag is cleared.
    copy_movie(&root, "Big Test (2020)/Big Test (2020) - copy.mp4");
    let fourth = scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    assert_eq!(fourth.moved + fourth.unchanged + fourth.changed, 2, "{fourth:?}");
    let parts = media::parts_for_library(&db, lib).await.unwrap();
    assert!(parts.iter().all(|p| p.missing_since.is_none()), "{parts:?}");
}

#[tokio::test]
async fn unavailable_root_marks_nothing_missing() {
    let Some((db, scanner, dir)) = setup().await else { return };
    let cancel = CancellationToken::new();
    let root = dir.path().join("usb");
    copy_movie(&root, "Big Test (2020)/Big Test (2020).mp4");
    let lib = library(&db, LibraryKind::Movie, &root).await;
    scanner.scan(lib, &cancel, |_| {}).await.unwrap();

    // Simulate an unmounted drive: the mount point exists but is empty.
    std::fs::remove_dir_all(root.join("Big Test (2020)")).unwrap();
    let stats = scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    assert_eq!(stats.unavailable_roots.len(), 1);
    assert_eq!(stats.missing, 0);
    let parts = media::parts_for_library(&db, lib).await.unwrap();
    assert!(parts[0].missing_since.is_none());
}

#[tokio::test]
async fn multi_part_movie_is_one_version() {
    let Some((db, scanner, dir)) = setup().await else { return };
    let cancel = CancellationToken::new();
    let root = dir.path().join("lib");
    copy_movie(&root, "Kill Bill (2003)/Kill Bill (2003) - cd1.mp4");
    copy_movie(&root, "Kill Bill (2003)/Kill Bill (2003) - cd2.mp4");
    let lib = library(&db, LibraryKind::Movie, &root).await;
    let stats = scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    assert_eq!(stats.added, 2, "{stats:?}");
    assert_eq!(count(&db, lib, MediaKind::Movie).await, 1);
    let versions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_versions")
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(versions, 1);
}

#[tokio::test]
async fn scans_external_sidecar_subtitles() {
    let Some((db, scanner, dir)) = setup().await else { return };
    let cancel = CancellationToken::new();
    let root = dir.path().join("lib");
    let movie = copy_movie(&root, "Movie (2020)/Movie.mp4");
    let movie_dir = movie.parent().unwrap();
    std::fs::write(
        movie_dir.join("Movie.en.srt"),
        b"1\n00:00:01,000 --> 00:00:02,000\nEnglish subtitle\n",
    )
    .unwrap();
    std::fs::write(
        movie_dir.join("Movie.en.forced.srt"),
        b"1\n00:00:01,000 --> 00:00:02,000\nForced subtitle\n",
    )
    .unwrap();
    std::fs::write(
        movie_dir.join("Movie.eng.sdh.srt"),
        b"1\n00:00:01,000 --> 00:00:02,000\n[SDH subtitle]\n",
    )
    .unwrap();

    let lib = library(&db, LibraryKind::Movie, &root).await;
    let stats = scanner.scan(lib, &cancel, |_| {}).await.unwrap();
    assert_eq!(stats.added, 1, "{stats:?}");

    let parts = media::parts_for_library(&db, lib).await.unwrap();
    assert_eq!(parts.len(), 1);
    let rows: Vec<_> = media::streams_for_part(&db, parts[0].id)
        .await
        .unwrap()
        .into_iter()
        .filter(|s| s.external_path.is_some())
        .collect();

    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0].codec, "srt");
    assert_eq!(rows[0].language.as_deref(), Some("en"));
    assert!(rows[0].is_forced);
    assert!(rows[0].external_path.as_deref().unwrap().ends_with("Movie.en.forced.srt"));

    assert_eq!(rows[1].codec, "srt");
    assert_eq!(rows[1].language.as_deref(), Some("en"));
    assert!(!rows[1].is_forced);
    assert!(rows[1].external_path.as_deref().unwrap().ends_with("Movie.en.srt"));

    assert_eq!(rows[2].codec, "srt");
    assert_eq!(rows[2].language.as_deref(), Some("eng"));
    assert_eq!(rows[2].title.as_deref(), Some("SDH"));
    assert!(!rows[2].is_forced);
    assert!(rows[2].external_path.as_deref().unwrap().ends_with("Movie.eng.sdh.srt"));
}
