//! Integration tests for Milestone M12 & Bugfixes:
//! - M12.1: Plugin toggle lifecycle (disable and re-enable plugins, verify manifest enabled status)
//! - M12.2: Chromecast & Remote Playback integration with PlayerRegistry (registration, heartbeats, commands, teardown)
//! - M12.3: Edge case and error handling verification (invalid plugin IDs, unauthorized calls)

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn m12_plugin_disable_and_reenable_lifecycle() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // Demo providers require explicit opt-in; exercise enable/disable independently of defaults.
    for id in ["youtube", "spotify", "webdrama"] {
        let result = app
            .call(
                Method::PATCH,
                &format!("/api/v1/plugins/{id}"),
                Some(&admin_token),
                Some(json!({"enabled":true,"settings":{}})),
            )
            .await;
        assert_eq!(result.status, StatusCode::OK);
    }
    // 1. Initial list of explicitly enabled plugins
    let list_res = app.call(Method::GET, "/api/v1/plugins", Some(&admin_token), None).await;
    assert_eq!(list_res.status, StatusCode::OK);
    let plugins = list_res.body.as_array().expect("plugins array");
    assert!(plugins.len() >= 3);

    for p in plugins {
        assert_eq!(
            p["enabled"].as_bool(),
            Some(true),
            "plugin {} should be enabled by default",
            p["id"]
        );
    }

    // 2. Feeds for YouTube should return channels initially
    let feeds_res =
        app.call(Method::GET, "/api/v1/plugins/youtube/channels", Some(&admin_token), None).await;
    assert_eq!(feeds_res.status, StatusCode::OK);
    assert!(feeds_res.body.as_array().expect("channels array").len() >= 2);

    // 3. Disable YouTube plugin
    let disable_res = app
        .call(
            Method::PATCH,
            "/api/v1/plugins/youtube",
            Some(&admin_token),
            Some(json!({
                "enabled": false,
                "settings": {}
            })),
        )
        .await;
    assert_eq!(disable_res.status, StatusCode::OK);
    assert_eq!(disable_res.body["enabled"].as_bool(), Some(false));

    // 4. Verify GET /api/v1/plugins reflects enabled == false for YouTube
    let list_res2 = app.call(Method::GET, "/api/v1/plugins", Some(&admin_token), None).await;
    assert_eq!(list_res2.status, StatusCode::OK);
    let plugins2 = list_res2.body.as_array().expect("plugins array");
    let yt = plugins2.iter().find(|p| p["id"] == "youtube").expect("youtube plugin found");
    assert_eq!(yt["enabled"].as_bool(), Some(false));

    // Other plugins must remain enabled
    let sp = plugins2.iter().find(|p| p["id"] == "spotify").expect("spotify plugin found");
    assert_eq!(sp["enabled"].as_bool(), Some(true));

    // 5. Disabled plugin channel feeds should now return empty array
    let feeds_disabled =
        app.call(Method::GET, "/api/v1/plugins/youtube/channels", Some(&admin_token), None).await;
    assert_eq!(feeds_disabled.status, StatusCode::OK);
    assert_eq!(feeds_disabled.body.as_array().expect("channels array").len(), 0);

    // 6. Re-enable YouTube plugin (Fix for user-reported bug: "när man disablat plugins kan man inte starta dem igen")
    let reenable_res = app
        .call(
            Method::PATCH,
            "/api/v1/plugins/youtube",
            Some(&admin_token),
            Some(json!({
                "enabled": true,
                "settings": {}
            })),
        )
        .await;
    assert_eq!(reenable_res.status, StatusCode::OK);
    assert_eq!(reenable_res.body["enabled"].as_bool(), Some(true));

    // 7. Verify GET /api/v1/plugins reflects enabled == true again
    let list_res3 = app.call(Method::GET, "/api/v1/plugins", Some(&admin_token), None).await;
    assert_eq!(list_res3.status, StatusCode::OK);
    let plugins3 = list_res3.body.as_array().expect("plugins array");
    let yt_again = plugins3.iter().find(|p| p["id"] == "youtube").expect("youtube plugin found");
    assert_eq!(yt_again["enabled"].as_bool(), Some(true));

    // 8. Channels are active again
    let feeds_again =
        app.call(Method::GET, "/api/v1/plugins/youtube/channels", Some(&admin_token), None).await;
    assert_eq!(feeds_again.status, StatusCode::OK);
    assert!(feeds_again.body.as_array().expect("channels array").len() >= 2);
}

#[tokio::test]
async fn m12_chromecast_player_registry_integration() {
    let (app, _db, services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Register a Chromecast player
    let chromecast_id = "chromecast-living-room-1";
    let reg_res = app
        .call(
            Method::POST,
            "/api/v1/players",
            Some(&admin_token),
            Some(json!({
                "id": chromecast_id,
                "name": "Living Room Chromecast Ultra",
                "deviceType": "chromecast",
                "state": "playing",
                "nowPlayingItemId": null,
                "positionMs": 45000,
                "durationMs": 180000,
                "volume": 75
            })),
        )
        .await;
    assert_eq!(reg_res.status, StatusCode::NO_CONTENT);

    // 2. List active players - should include the Chromecast
    let list_res = app.call(Method::GET, "/api/v1/players", Some(&admin_token), None).await;
    assert_eq!(list_res.status, StatusCode::OK);
    let players = list_res.body.as_array().expect("players array");
    assert_eq!(players.len(), 1);

    let p = &players[0];
    assert_eq!(p["id"].as_str().unwrap(), chromecast_id);
    assert_eq!(p["deviceType"].as_str().unwrap(), "chromecast");
    assert_eq!(p["name"].as_str().unwrap(), "Living Room Chromecast Ultra");
    assert_eq!(p["state"].as_str().unwrap(), "playing");
    assert_eq!(p["positionMs"].as_i64().unwrap(), 45000);
    assert_eq!(p["volume"].as_i64().unwrap(), 75);

    let auth = services.auth.authenticate(&admin_token).await.unwrap();
    let mut connection =
        services.players.connect(&auth.actor, auth.session_id, chromecast_id).unwrap();
    // 3. Dispatch command to Chromecast
    let cmd_res = app
        .call(
            Method::POST,
            &format!("/api/v1/players/{chromecast_id}/command"),
            Some(&admin_token),
            Some(json!({
                "action": "pause"
            })),
        )
        .await;
    assert_eq!(cmd_res.status, StatusCode::NO_CONTENT);

    assert!(matches!(connection.commands.recv().await.unwrap(), mbl_service::PlayerCommand::Pause));
    // 4. Update state to paused via heartbeat
    let update_res = app
        .call(
            Method::POST,
            "/api/v1/players",
            Some(&admin_token),
            Some(json!({
                "id": chromecast_id,
                "name": "Living Room Chromecast Ultra",
                "deviceType": "chromecast",
                "state": "paused",
                "nowPlayingItemId": null,
                "positionMs": 45000,
                "durationMs": 180000,
                "volume": 75
            })),
        )
        .await;
    assert_eq!(update_res.status, StatusCode::NO_CONTENT);

    // 5. Unregister player on disconnect
    let del_res = app
        .call(Method::DELETE, &format!("/api/v1/players/{chromecast_id}"), Some(&admin_token), None)
        .await;
    assert_eq!(del_res.status, StatusCode::NO_CONTENT);

    // 6. List players - should be empty now
    let list_after = app.call(Method::GET, "/api/v1/players", Some(&admin_token), None).await;
    assert_eq!(list_after.status, StatusCode::OK);
    assert_eq!(list_after.body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn m12_plugin_and_player_edge_cases() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Non-existent plugin should return 404, not 500
    let res_404 =
        app.call(Method::GET, "/api/v1/plugins/unknown_plugin", Some(&admin_token), None).await;
    assert_eq!(res_404.status, StatusCode::NOT_FOUND);

    // 2. Unauthorized access without bearer token should return 401
    let unauth = app.call(Method::GET, "/api/v1/plugins", None, None).await;
    assert_eq!(unauth.status, StatusCode::UNAUTHORIZED);

    let unauth_players = app.call(Method::GET, "/api/v1/players", None, None).await;
    assert_eq!(unauth_players.status, StatusCode::UNAUTHORIZED);

    // 3. Command to non-existent player should return 404
    let cmd_nonexistent = app
        .call(
            Method::POST,
            "/api/v1/players/nonexistent-id/command",
            Some(&admin_token),
            Some(json!({ "action": "pause" })),
        )
        .await;
    assert_eq!(cmd_nonexistent.status, StatusCode::NOT_FOUND);
}
