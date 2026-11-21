//! Database maintenance: settings, preview, cleanup of titles whose files are gone, backups.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use serde_json::json;

#[tokio::test]
async fn maintenance_previews_then_cleans_up_and_keeps_backups() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;

    let res = app.call(Method::GET, "/api/v1/system/maintenance", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app.call(Method::GET, "/api/v1/system/maintenance", Some(&admin), None).await;
    assert_eq!(res.body["enabled"], true);
    assert_eq!(res.body["hour"], 4);
    assert_eq!(res.body["cleanupAfterDays"], 0);

    let settings = |cleanup: u32| {
        json!({ "enabled": true, "hour": 3, "intervalDays": 1, "keepBackups": 2,
                "cleanupAfterDays": cleanup, "utcOffsetMinutes": 60 })
    };
    let res =
        app.call(Method::PUT, "/api/v1/system/maintenance", Some(&admin), Some(settings(3))).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let res =
        app.call(Method::PUT, "/api/v1/system/maintenance", Some(&admin), Some(settings(7))).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);

    // Heat's file has been gone 40 days; an empty show is left over; an old finished job.
    let long_ago = mbl_core::time::now_ms() - 40 * 86_400_000;
    sqlx::query(
        "UPDATE media_parts SET missing_since = ? WHERE version_id IN \
         (SELECT id FROM media_versions WHERE item_id = ?)",
    )
    .bind(long_ago)
    .bind(seeded.movie_id)
    .execute(&db.write)
    .await
    .unwrap();
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = ?")
        .bind(seeded.movie_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at) \
         VALUES (?, 'show', 'Empty show', 'empty show', 1, 1)",
    )
    .bind(library)
    .execute(&db.write)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO jobs (kind, payload, status, priority, run_after, created_at, updated_at) \
         VALUES ('x', '{}', 'done', 1, 0, 0, 0)",
    )
    .execute(&db.write)
    .await
    .unwrap();
    // Keep the library folder readable: the fixture's placeholder file is in it.
    let item = format!("/api/v1/items/{}", seeded.movie_id);

    // Preview: nothing changes.
    let run = |body: serde_json::Value| {
        let app = &app;
        let admin = admin.clone();
        async move {
            app.call(Method::POST, "/api/v1/system/maintenance/run", Some(&admin), Some(body)).await
        }
    };
    let res = run(json!({ "dryRun": true, "cleanup": true })).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["goneItems"], 1, "{}", res.body);
    assert_eq!(res.body["examples"], json!(["Heat"]));
    assert_eq!(res.body["emptyItems"], 1);
    assert_eq!(res.body["oldJobs"], 1);
    assert_eq!(res.body["goneDays"], 7);
    assert_eq!(res.body["cleaned"], false);
    assert_eq!(app.call(Method::GET, &item, Some(&admin), None).await.status, StatusCode::OK);

    // Without cleanup the title stays.
    let res = run(json!({})).await;
    assert_eq!(res.body["cleaned"], false);
    assert_eq!(app.call(Method::GET, &item, Some(&admin), None).await.status, StatusCode::OK);

    // With cleanup it goes, together with the empty show and the old job.
    let res = run(json!({ "cleanup": true })).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["cleaned"], true);
    assert_eq!(res.body["vacuumed"], true);
    assert_eq!(
        app.call(Method::GET, &item, Some(&admin), None).await.status,
        StatusCode::NOT_FOUND
    );
    let empty: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE title = 'Empty show'")
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert_eq!(empty, 0);

    // Two automatic backups are kept.
    let res = run(json!({})).await;
    assert!(res.body["backupsRemoved"].as_u64().unwrap() >= 1, "{}", res.body);
    let backups = std::fs::read_dir(app.dir.path().join("data").join("backups"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("mbl-auto-"))
        .count();
    assert_eq!(backups, 2);
    let res = app.call(Method::GET, "/api/v1/system/maintenance", Some(&admin), None).await;
    assert!(res.body["lastRun"].as_i64().is_some());
    assert_eq!(res.body["lastReport"]["dryRun"], false);
}
