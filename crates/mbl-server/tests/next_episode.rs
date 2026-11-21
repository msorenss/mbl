//! The play button of a show or season page: which episode it starts.

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;

#[tokio::test]
async fn show_pages_play_the_episode_after_the_last_one_watched() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;
    // Give episode 2 a file too, so both can be played.
    let version: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mkv', 'h264', 'aac', 1920, 1080) RETURNING id",
    )
    .bind(fixture.ep2_id)
    .fetch_one(&db.write)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, probe, chapters, part_index)
         VALUES (?, ?, 2048, 1, 1500000, '{}', '[]', 0)",
    )
    .bind(version)
    .bind(std::path::Path::new(&app.media_dir("Movies")).join("Show - S01E02.mkv").to_str().unwrap())
    .execute(&db.write)
    .await
    .unwrap();
    let next = |id: i64| {
        let uri = format!("/api/v1/items/{id}/next-episode");
        let app = &app;
        let admin = admin.clone();
        async move { app.call(Method::GET, &uri, Some(&admin), None).await }
    };

    // Nothing watched: the first episode from the start, from the show and the season page.
    for id in [fixture.show_id, fixture.season_id] {
        let res = next(id).await;
        assert_eq!(res.status, StatusCode::OK, "{}", res.body);
        assert_eq!(res.body["episode"]["id"], fixture.ep1_id);
        assert_eq!(res.body["startMs"], 0);
    }

    // Episode 1 finished: episode 2.
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/watched", fixture.ep1_id),
            Some(&admin),
            None,
        )
        .await;
    assert!(res.status.is_success(), "{}", res.body);
    let res = next(fixture.show_id).await;
    assert_eq!(res.body["episode"]["id"], fixture.ep2_id, "{}", res.body);

    // Halfway through episode 2: resume it.
    let user: i64 =
        sqlx::query_scalar("SELECT MIN(id) FROM users").fetch_one(&db.read).await.unwrap();
    sqlx::query(
        "INSERT INTO watch_state (user_id, item_id, view_offset_ms, view_count, last_viewed_at)
         VALUES (?, ?, 600000, 0, 1) ON CONFLICT(user_id, item_id) DO UPDATE SET view_offset_ms = 600000",
    )
    .bind(user)
    .bind(fixture.ep2_id)
    .execute(&db.write)
    .await
    .unwrap();
    let res = next(fixture.show_id).await;
    assert_eq!(res.body["episode"]["id"], fixture.ep2_id);
    assert_eq!(res.body["startMs"], 600000);

    // A movie has no episodes; others cannot see the show.
    let res = next(fixture.movie_id).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let other = app.create_user(&admin, "other").await;
    let res = app
        .call(Method::GET, &format!("/api/v1/items/{}/next-episode", 999_999), Some(&other), None)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}
