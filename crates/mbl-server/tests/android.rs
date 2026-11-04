//! Server prerequisites for the Android client (M11.1 S1–S7).
#![allow(clippy::unwrap_used)]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{TestApp, seed_playback_fixtures};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn admin_user_id(db: &mbl_db::Db) -> i64 {
    sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(&db.read)
        .await
        .unwrap()
}

#[tokio::test]
async fn account_self_deletion_requires_password_and_keeps_last_admin() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let user = app.create_user(&admin, "alice").await;

    let wrong = app
        .call(Method::DELETE, "/api/v1/users/me", Some(&user), Some(json!({ "password": "nope" })))
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED, "{:?}", wrong.body);

    let last_admin = app
        .call(
            Method::DELETE,
            "/api/v1/users/me",
            Some(&admin),
            Some(json!({ "password": "admin-password" })),
        )
        .await;
    assert_eq!(last_admin.status, StatusCode::CONFLICT, "{:?}", last_admin.body);

    let ok = app
        .call(
            Method::DELETE,
            "/api/v1/users/me",
            Some(&user),
            Some(json!({ "password": "alice-password" })),
        )
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT, "{:?}", ok.body);

    let me = app.call(Method::GET, "/api/v1/auth/me", Some(&user), None).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.login("alice", "alice-password").await.status, StatusCode::UNAUTHORIZED);
    let me = app.call(Method::GET, "/api/v1/auth/me", Some(&admin), None).await;
    assert_eq!(me.status, StatusCode::OK);
}

#[tokio::test]
async fn offline_progress_sync_applies_only_newer_entries() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let uid = admin_user_id(&db).await;
    let now = mbl_core::time::now_ms();

    let res = app
        .call(
            Method::POST,
            "/api/v1/users/me/progress/sync",
            Some(&admin),
            Some(json!({ "entries": [
                { "itemId": seeded.movie_id, "positionMs": 120_000, "updatedAt": now - 10_000 },
                { "itemId": seeded.ep1_id, "played": true, "updatedAt": now - 10_000 },
                { "itemId": 999_999, "positionMs": 1, "updatedAt": now },
            ] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let outcomes: Vec<&str> = res.body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, ["applied", "applied", "not_found"]);

    // An older offline entry must not overwrite the newer server state.
    let stale = app
        .call(
            Method::POST,
            "/api/v1/users/me/progress/sync",
            Some(&admin),
            Some(json!({ "entries": [
                { "itemId": seeded.movie_id, "positionMs": 5_000, "updatedAt": now - 60_000 },
            ] })),
        )
        .await;
    assert_eq!(stale.body["results"][0]["outcome"], "stale", "{:?}", stale.body);

    let (offset, count): (i64, i64) = sqlx::query_as(
        "SELECT view_offset_ms, view_count FROM watch_state WHERE user_id = ? AND item_id = ?",
    )
    .bind(uid)
    .bind(seeded.movie_id)
    .fetch_one(&db.read)
    .await
    .unwrap();
    assert_eq!((offset, count), (120_000, 0));
    let watched: i64 =
        sqlx::query_scalar("SELECT view_count FROM watch_state WHERE user_id = ? AND item_id = ?")
            .bind(uid)
            .bind(seeded.ep1_id)
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert_eq!(watched, 1);

    let invalid = app
        .call(
            Method::POST,
            "/api/v1/users/me/progress/sync",
            Some(&admin),
            Some(json!({ "entries": [{ "itemId": seeded.movie_id, "positionMs": 1, "updatedAt": 0 }] })),
        )
        .await;
    assert!(invalid.status.is_client_error(), "{:?}", invalid.body);
}

#[tokio::test]
async fn lyrics_prefer_lrc_sidecar_and_require_a_track() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let music = app.media_dir("Music");
    let lib = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(json!({ "name": "Music", "kind": "music", "paths": [music] })),
        )
        .await;
    assert_eq!(lib.status, StatusCode::CREATED, "{:?}", lib.body);
    let lib_id = lib.body["id"].as_i64().unwrap();
    let audio = std::path::Path::new(&music).join("Song.flac");
    std::fs::write(&audio, b"fLaC").unwrap();
    std::fs::write(audio.with_extension("lrc"), "[ar:Band]\n[00:01.50]Hello\n[00:03.00]World\n")
        .unwrap();

    let track: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'track', 'Song', 'song', 1, 1) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();
    let version: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, audio_codec) VALUES (?, 'flac', 'flac') RETURNING id",
    )
    .bind(track)
    .fetch_one(&db.write)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, probe, chapters, part_index)
         VALUES (?, ?, 4, 1, 5000, '{}', '[]', 0)",
    )
    .bind(version)
    .bind(audio.to_str().unwrap())
    .execute(&db.write)
    .await
    .unwrap();

    let res =
        app.call(Method::GET, &format!("/api/v1/items/{track}/lyrics"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["synced"], true);
    assert_eq!(res.body["source"], "sidecar");
    assert_eq!(res.body["lines"][0], json!({ "startMs": 1500, "text": "Hello" }));
    assert_eq!(res.body["lines"][1]["text"], "World");

    let anonymous =
        app.call(Method::GET, &format!("/api/v1/items/{track}/lyrics"), None, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let missing = app.call(Method::GET, "/api/v1/items/999999/lyrics", Some(&admin), None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn endpoints_document_is_public_and_matches_server_id() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let res = app.call(Method::GET, "/api/v1/system/endpoints", None, None).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let id = res.body["serverId"].as_str().unwrap();
    assert_eq!(id.len(), 32);
    assert!(res.body["lanUrls"].is_array());
    assert!(res.body["tailnetUrls"].is_array());
    let info = app.call(Method::GET, "/api/v1/system/info", Some(&admin), None).await;
    assert_eq!(info.body["serverId"], id);
    // The stable id survives a second lookup.
    let again = app.call(Method::GET, "/api/v1/system/endpoints", None, None).await;
    assert_eq!(again.body["serverId"], id);
    let text = res.body.to_string().to_lowercase();
    for secret in ["key", "password", "token", "secret"] {
        assert!(!text.contains(secret), "endpoints leaked {secret}: {text}");
    }
}

#[tokio::test]
async fn original_downloads_are_owned_resumable_and_deletable() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;

    let created = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/downloads", seeded.movie_id),
            Some(&admin),
            Some(json!({ "quality": "original" })),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "{:?}", created.body);
    assert_eq!(created.body["status"], "ready");
    assert_eq!(created.body["partId"], seeded.movie_part_id);
    assert_eq!(created.body["fileName"], "Heat (1995).mp4");
    assert_eq!(created.body["mimeType"], "video/mp4");
    let id = created.body["id"].as_i64().unwrap();
    let size = created.body["sizeBytes"].as_i64().unwrap();
    assert_eq!(size, b"synthetic mp4 content".len() as i64);

    // Idempotent per part and quality.
    let again = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/downloads", seeded.movie_id),
            Some(&admin),
            Some(json!({ "quality": "original" })),
        )
        .await;
    assert_eq!(again.body["id"], id);
    let list = app.call(Method::GET, "/api/v1/downloads", Some(&admin), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);

    let ranged = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/downloads/{id}/file"))
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .header(header::RANGE, "bytes=10-")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
    let disposition =
        ranged.headers().get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap().to_owned();
    assert!(disposition.starts_with("attachment;"), "{disposition}");
    let body = ranged.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], &b"synthetic mp4 content"[10..]);

    // Another user can neither see nor fetch the ticket.
    let bob = app.create_user(&admin, "bob").await;
    for uri in [format!("/api/v1/downloads/{id}"), format!("/api/v1/downloads/{id}/file")] {
        let res = app.call(Method::GET, &uri, Some(&bob), None).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{uri}");
    }
    assert_eq!(
        app.call(Method::DELETE, &format!("/api/v1/downloads/{id}"), Some(&bob), None).await.status,
        StatusCode::NOT_FOUND
    );
    let anonymous =
        app.call(Method::GET, &format!("/api/v1/downloads/{id}/file"), None, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

    let deleted =
        app.call(Method::DELETE, &format!("/api/v1/downloads/{id}"), Some(&admin), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let gone = app.call(Method::GET, &format!("/api/v1/downloads/{id}"), Some(&admin), None).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    // The source media file is never removed by deleting an original download.
    assert!(std::path::Path::new(&app.media_dir("Movies")).join("Heat (1995).mp4").is_file());
}

#[tokio::test]
async fn transcoded_downloads_are_queued_once_and_not_served_early() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let uri = format!("/api/v1/items/{}/downloads", seeded.movie_id);

    let first = app
        .call(Method::POST, &uri, Some(&admin), Some(json!({ "quality": "universal_720p" })))
        .await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.body);
    assert_eq!(first.body["fileName"], "Heat (1995).720p.mp4");
    let status = first.body["status"].as_str().unwrap().to_owned();
    assert!(["pending", "transcoding", "failed"].contains(&status.as_str()), "{status}");
    let id = first.body["id"].as_i64().unwrap();

    let second = app
        .call(Method::POST, &uri, Some(&admin), Some(json!({ "quality": "universal_720p" })))
        .await;
    assert_eq!(second.body["id"], id);
    let jobs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'download.transcode'")
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert!(jobs >= 1);

    let ticket =
        app.call(Method::GET, &format!("/api/v1/downloads/{id}"), Some(&admin), None).await;
    if ticket.body["status"] != "ready" {
        let file = app
            .call(Method::GET, &format!("/api/v1/downloads/{id}/file"), Some(&admin), None)
            .await;
        assert_eq!(file.status, StatusCode::CONFLICT, "{:?}", file.body);
    }

    let bad = app.call(Method::POST, &uri, Some(&admin), Some(json!({ "quality": "8k" }))).await;
    assert!(bad.status.is_client_error(), "{:?}", bad.status);
}
