//! Regressions found while reviewing the v1 web and shared-service flows.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn shared_membership_honors_library_and_parental_access() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let mut libraries = Vec::new();
    for name in ["Family", "Private"] {
        let result = app
            .call(
                Method::POST,
                "/api/v1/libraries",
                Some(&admin),
                Some(json!({"name": name, "kind": "movie", "paths": [app.media_dir(name)]})),
            )
            .await;
        assert_eq!(result.status, StatusCode::CREATED);
        libraries.push(result.body["id"].as_i64().unwrap());
    }
    let mut ids = Vec::new();
    for (lib, title, rating) in [
        (libraries[0], "Allowed", "G"),
        (libraries[0], "Mature", "R"),
        (libraries[1], "Private", "G"),
    ] {
        let id: i64 = sqlx::query_scalar("INSERT INTO media_items (library_id, kind, title, sort_title, content_rating, added_at, updated_at) VALUES (?, 'movie', ?, ?, ?, 100, 100) RETURNING id")
            .bind(lib).bind(title).bind(title).bind(rating).fetch_one(&db.write).await.unwrap();
        ids.push(id);
    }
    let child_result = app.call(Method::POST, "/api/v1/users", Some(&admin), Some(json!({
        "username": "child", "password": "child-password", "libraryIds": [libraries[0]], "contentRatingCeiling": "G"
    }))).await;
    assert_eq!(child_result.status, StatusCode::CREATED);
    let login = app.login("child", "child-password").await;
    let child = login.body["token"].as_str().unwrap();
    for (path, body) in [
        ("collections", json!({"name": "Shared"})),
        ("playlists", json!({"name": "Shared", "isPublic": true})),
    ] {
        let created =
            app.call(Method::POST, &format!("/api/v1/{path}"), Some(&admin), Some(body)).await;
        assert_eq!(created.status, StatusCode::CREATED);
        let id = created.body["id"].as_i64().unwrap();
        for item in &ids {
            assert_eq!(
                app.call(
                    Method::POST,
                    &format!("/api/v1/{path}/{id}/items/{item}"),
                    Some(&admin),
                    None
                )
                .await
                .status,
                StatusCode::NO_CONTENT
            );
        }
        let result =
            app.call(Method::GET, &format!("/api/v1/{path}/{id}/items"), Some(child), None).await;
        assert_eq!(result.status, StatusCode::OK);
        assert_eq!(result.body, json!([ids[0]]));
        let detail =
            app.call(Method::GET, &format!("/api/v1/{path}/{id}"), Some(child), None).await;
        assert_eq!(detail.body["itemCount"], 1);
        let list = app.call(Method::GET, &format!("/api/v1/{path}"), Some(child), None).await;
        assert_eq!(list.body[0]["itemCount"], 1);
        let admin_result =
            app.call(Method::GET, &format!("/api/v1/{path}/{id}/items"), Some(&admin), None).await;
        assert_eq!(admin_result.body, json!(ids));
    }
    let playlist = app
        .call(Method::POST, "/api/v1/playlists", Some(child), Some(json!({"name": "Mine"})))
        .await;
    let id = playlist.body["id"].as_i64().unwrap();
    for hidden in &ids[1..] {
        assert_eq!(
            app.call(
                Method::POST,
                &format!("/api/v1/playlists/{id}/items/{hidden}"),
                Some(child),
                None
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        app.call(Method::GET, "/api/v1/collections/999999/items", Some(child), None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn totp_setup_does_not_disable_an_enabled_factor() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let setup = app.call(Method::POST, "/api/v1/auth/totp/setup", Some(&admin), None).await;
    let secret = setup.body["secret"].as_str().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let code = mbl_auth::totp::compute_code(secret, now).unwrap();
    let verified = app
        .call(Method::POST, "/api/v1/auth/totp/verify", Some(&admin), Some(json!({"pin": code})))
        .await;
    assert_eq!(verified.body, json!(true));
    assert_eq!(
        app.call(Method::POST, "/api/v1/auth/totp/setup", Some(&admin), None).await.status,
        StatusCode::CONFLICT
    );
    assert_eq!(app.login("admin", "admin-password").await.status, StatusCode::UNAUTHORIZED);
    let result = app
        .call(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "admin", "password": "admin-password", "totpCode": code})),
        )
        .await;
    assert_eq!(result.status, StatusCode::OK);
}

#[tokio::test]
async fn dvr_checks_all_channels_padding_and_peak_concurrency() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let tuner =
        mbl_db::repo::m9::create_tuner(&db, "Single tuner", "m3u", "http://localhost/list", 1)
            .await
            .unwrap();
    let mut channels = Vec::new();
    for name in ["One", "Two"] {
        channels.push(
            mbl_db::repo::m9::upsert_channel(
                &db,
                Some(tuner),
                name,
                name,
                None,
                None,
                "http://localhost/live.m3u8",
                None,
                false,
            )
            .await
            .unwrap(),
        );
    }
    let start = 1_800_000_000_000_i64;
    let schedule = |channel: mbl_core::ChannelId, from, to, padding| {
        json!({
            "channelId": channel.0, "title": "Programme", "startTime": from, "endTime": to,
            "startPaddingSecs": padding
        })
    };
    let first = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(schedule(channels[0], start, start + 60_000, 0)),
        )
        .await;
    assert_eq!(first.status, StatusCode::OK);
    let conflict = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(schedule(channels[1], start + 30_000, start + 90_000, 0)),
        )
        .await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    let padding_conflict = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(schedule(channels[1], start + 70_000, start + 90_000, 20)),
        )
        .await;
    assert_eq!(padding_conflict.status, StatusCode::CONFLICT);
    let second = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(schedule(channels[1], start + 60_000, start + 120_000, 0)),
        )
        .await;
    assert_eq!(second.status, StatusCode::OK);
    // Two sequential reservations have peak concurrency one, not two.
    assert_eq!(
        mbl_db::repo::m9::count_overlapping_recordings(&db, channels[0], start, start + 120_000)
            .await
            .unwrap(),
        1
    );
    let invalid = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(schedule(channels[0], start + 100, start, 0)),
        )
        .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn playback_rejects_a_part_from_another_item() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seed = common::seed_playback_fixtures(&app, &db, &admin).await;
    let result = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId": seed.ep1_id, "partId": seed.movie_part_id, "profile": "chrome"})),
        )
        .await;
    assert_eq!(result.status, StatusCode::NOT_FOUND);
}
