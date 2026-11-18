//! Background intro/credits analysis: pending selection, chapter-based markers and the record
//! that keeps an analysed file from being analysed again.

mod common;

use common::{TestApp, seed_playback_fixtures};
use mbl_core::{ItemId, LibraryId};
use mbl_db::repo::m5::MarkersRepo;

#[tokio::test]
async fn analysis_finds_markers_and_is_not_repeated() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let s = seed_playback_fixtures(&app, &db, &admin).await;

    // Real files at the seeded paths; the episode gets intro and credits chapters.
    for part in [s.ep1_part_id, s.movie_part_id] {
        let path: String = sqlx::query_scalar("SELECT path FROM media_parts WHERE id = ?")
            .bind(part)
            .fetch_one(&db.read)
            .await
            .unwrap();
        std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
    }
    let chapters = serde_json::json!([
        { "startMs": 0, "endMs": 30000, "title": "Intro" },
        { "startMs": 30000, "endMs": 2500000, "title": "Episode" },
        { "startMs": 2500000, "endMs": 2600000, "title": "End Credits" }
    ]);
    sqlx::query("UPDATE media_parts SET chapters = ?, duration_ms = 2600000 WHERE id = ?")
        .bind(chapters.to_string())
        .bind(s.ep1_part_id)
        .execute(&db.write)
        .await
        .unwrap();

    let repo = MarkersRepo::new(db.clone());
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = ?")
        .bind(s.ep1_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let pending = repo.pending_marker_analysis(LibraryId(library), 50).await.unwrap();
    assert!(pending.contains(&(ItemId(s.season_id), "season".to_owned())), "{pending:?}");

    let markers = services.intelligence.analyze_season_markers(ItemId(s.season_id)).await.unwrap();
    let ep1: Vec<_> = markers.iter().filter(|m| m.item_id == ItemId(s.ep1_id)).collect();
    assert!(ep1.iter().any(|m| m.kind == "intro" && m.start_ms == 0 && m.end_ms == 30000));
    assert!(ep1.iter().any(|m| m.kind == "credits" && m.start_ms == 2_500_000));

    let pending = repo.pending_marker_analysis(LibraryId(library), 50).await.unwrap();
    assert!(!pending.contains(&(ItemId(s.season_id), "season".to_owned())), "{pending:?}");

    // The movie: credits from its seeded "End Credits" chapter, then no longer pending.
    let movie_library: i64 = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = ?")
        .bind(s.movie_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let movie = (ItemId(s.movie_id), "movie".to_owned());
    assert!(
        repo.pending_marker_analysis(LibraryId(movie_library), 50).await.unwrap().contains(&movie)
    );
    services.intelligence.analyze_movie_markers(ItemId(s.movie_id)).await.unwrap();
    let movie_markers = repo.list_for_item(ItemId(s.movie_id)).await.unwrap();
    assert!(movie_markers.iter().any(|m| m.kind == "credits" && m.start_ms == 5_700_000));
    assert!(
        !repo.pending_marker_analysis(LibraryId(movie_library), 50).await.unwrap().contains(&movie)
    );

    // Without the record (a replaced file has a new part id) the movie is pending again.
    sqlx::query("DELETE FROM marker_analysis WHERE item_id = ?")
        .bind(s.movie_id)
        .execute(&db.write)
        .await
        .unwrap();
    assert!(
        repo.pending_marker_analysis(LibraryId(movie_library), 50).await.unwrap().contains(&movie)
    );
}
