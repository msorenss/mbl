//! Remote control of a web player, movie night, library scene indexing and channel edits.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use serde_json::json;

#[tokio::test]
async fn another_device_of_the_same_user_controls_a_web_player() {
    let (app, _db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;
    let phone =
        app.login("viewer", "viewer-password").await.body["token"].as_str().unwrap().to_owned();

    let player = json!({
        "id": "web-1", "name": "Firefox on Linux", "deviceType": "web", "state": "playing",
        "nowPlayingItemId": null, "positionMs": 1000, "durationMs": 60000, "volume": 80
    });
    let res = app.call(Method::POST, "/api/v1/players", Some(&viewer), Some(player)).await;
    assert!(res.status.is_success(), "{}", res.body);
    let auth = services.auth.authenticate(&viewer).await.unwrap();
    let mut connection = services.players.connect(&auth.actor, auth.session_id, "web-1").unwrap();

    // The phone is a different session of the same user and sees the browser.
    let res = app.call(Method::GET, "/api/v1/players", Some(&phone), None).await;
    assert_eq!(res.body[0]["id"], "web-1");
    let res = app
        .call(
            Method::POST,
            "/api/v1/players/web-1/command",
            Some(&phone),
            Some(json!({ "action": "seek", "params": { "positionMs": 31000 } })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
    assert!(matches!(
        connection.commands.recv().await.unwrap(),
        mbl_service::PlayerCommand::Seek { position_ms: 31000 }
    ));

    // Other users neither see nor control it.
    let other = app.create_user(&admin, "other").await;
    let res = app.call(Method::GET, "/api/v1/players", Some(&other), None).await;
    assert_eq!(res.body, json!([]));
    let res = app
        .call(
            Method::POST,
            "/api/v1/players/web-1/command",
            Some(&other),
            Some(json!({ "action": "pause" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn movie_night_counts_members_and_matches_on_a_shared_like() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let friend = app.create_user(&admin, "friend").await;

    let res = app
        .call(
            Method::POST,
            "/api/v1/matchmaker/sessions",
            Some(&admin),
            Some(json!({ "filterConfig": "{\"libraryId\":1}" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    let id = res.body["id"].as_str().unwrap().to_owned();
    let path = format!("/api/v1/matchmaker/sessions/{id}");
    assert_eq!(app.call(Method::GET, &path, Some(&admin), None).await.body["memberCount"], 1);

    // Not a member yet: the session is hidden.
    assert_eq!(
        app.call(Method::GET, &path, Some(&friend), None).await.status,
        StatusCode::NOT_FOUND
    );
    let res = app.call(Method::POST, &format!("{path}/join"), Some(&friend), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
    assert_eq!(app.call(Method::GET, &path, Some(&friend), None).await.body["memberCount"], 2);

    let vote = |like: i32| json!({ "userName": "", "itemId": seeded.movie_id, "vote": like });
    let res = app.call(Method::POST, &format!("{path}/vote"), Some(&admin), Some(vote(1))).await;
    assert_eq!(res.body["matched"], false, "{}", res.body);
    let res = app.call(Method::POST, &format!("{path}/vote"), Some(&friend), Some(vote(1))).await;
    assert_eq!(res.body["matched"], true, "{}", res.body);
    assert_eq!(res.body["matchedItemId"], seeded.movie_id);
    let res = app.call(Method::GET, &path, Some(&friend), None).await;
    assert_eq!(res.body["status"], "matched");
}

#[tokio::test]
async fn administrators_queue_scene_indexing_for_a_library() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    seed_playback_fixtures(&app, &db, &admin).await;
    let libraries = app.call(Method::GET, "/api/v1/libraries", Some(&admin), None).await;
    let movies = libraries.body.as_array().unwrap().iter().find(|l| l["kind"] == "movie").unwrap();
    let path = format!("/api/v1/libraries/{}/index-scenes", movies["id"]);

    let res = app.call(Method::POST, &path, Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::ACCEPTED, "{}", res.body);
    assert!(res.body["pendingItems"].as_u64().unwrap() >= 1, "{}", res.body);

    let viewer = app.create_user(&admin, "viewer").await;
    assert_eq!(
        app.call(Method::POST, &path, Some(&viewer), None).await.status,
        StatusCode::FORBIDDEN
    );
    let res =
        app.call(Method::POST, "/api/v1/libraries/9999/index-scenes", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // Search takes its text as a query parameter.
    let res =
        app.call(Method::GET, "/api/v1/ai/search-scenes?q=car&limit=5", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
}

#[tokio::test]
async fn channel_edits_are_validated_and_an_empty_guide_id_unlinks() {
    let (app, _db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let res = app
        .call(
            Method::POST,
            "/api/v1/livetv/tuners/m3u",
            Some(&admin),
            Some(json!({
                "name": "IPTV",
                "content": "#EXTM3U\n#EXTINF:-1 tvg-id=\"one.se\",One\nhttps://live.example.com/one.m3u8\n"
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let channels = app.call(Method::GET, "/api/v1/livetv/channels", Some(&admin), None).await;
    let path = format!("/api/v1/livetv/channels/{}", channels.body[0]["id"]);

    let res = app.call(Method::PATCH, &path, Some(&admin), Some(json!({ "name": "  " }))).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let res = app
        .call(
            Method::PATCH,
            &path,
            Some(&admin),
            Some(json!({ "name": "Channel One", "epgChannelId": "", "isHidden": true })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["name"], "Channel One");
    assert!(res.body["epgChannelId"].is_null());
    assert_eq!(res.body["isHidden"], true);
}
