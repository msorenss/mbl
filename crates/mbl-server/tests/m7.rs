//! Integration tests for Milestone M7: Media Breadth
//! - M7.1: Music scanning & MusicBrainz client
//! - M7.2: Audio loudness analysis & progressive audio streaming
//! - M7.4: OpenSubsonic API compatibility
//! - M7.5: Photos EXIF and timeline breakdown
//! - M7.6: Collections and Playlists CRUD
//! - M7.8: OpenSubtitles client search
//! - M7.9 & M7.15: DLNA UPnP XML and Home Assistant MQTT Discovery
//! - M7.10 & M7.13: Player Registry & Remote Control
//! - M7.11: Webhook notifications
//! - M7.16: AI Watch Companion live context & reactions
//! - M7.17: Subtitle auto-sync
//! - M7.18: Scene X-Ray and spoiler-free recap logic
//! - M7.19: Movie Night Matchmaker
//! - M7.20: Built-in Media Requests and hardlink storage analysis

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_analysis::features::build_spoiler_free_recap;
use mbl_analysis::loudness::parse_ebur128_output;
use mbl_analysis::subsync::align_srt_content;
use mbl_remote::dlna::generate_device_description;
use mbl_remote::mqtt::{IncomingPlayerCommand, build_player_discovery, parse_incoming_command};
use mbl_service::players::PlayerInfo;
use serde_json::json;

#[tokio::test]
async fn m7_collections_and_playlists_lifecycle() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Create a Library and Item
    let dir = app.media_dir("Music");
    let res = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_token),
            Some(json!({ "name": "Music", "kind": "music", "paths": [dir] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let lib_id = res.body["id"].as_i64().unwrap();

    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'track', 'Bohemian Rhapsody', 'bohemian rhapsody', 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&_db.write)
    .await
    .unwrap();

    // 2. Create Collection
    let res = app
        .call(
            Method::POST,
            "/api/v1/collections",
            Some(&admin_token),
            Some(json!({ "name": "Classic Rock Essentials", "overview": "Greatest hits" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let col_id = res.body["id"].as_i64().unwrap();
    assert_eq!(res.body["name"], "Classic Rock Essentials");

    // 3. Add Item to Collection
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/collections/{col_id}/items/{item_id}"),
            Some(&admin_token),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    // 4. List Collections
    let res = app.call(Method::GET, "/api/v1/collections", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let cols = res.body.as_array().unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0]["itemCount"], 1);

    // 5. Create Playlist
    let res = app
        .call(
            Method::POST,
            "/api/v1/playlists",
            Some(&admin_token),
            Some(json!({ "name": "Driving Anthems", "isPublic": true })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let pl_id = res.body["id"].as_i64().unwrap();

    // 6. Add Item to Playlist
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/playlists/{pl_id}/items/{item_id}"),
            Some(&admin_token),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    // 7. List Playlists
    let res = app.call(Method::GET, "/api/v1/playlists", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let pls = res.body.as_array().unwrap();
    assert_eq!(pls.len(), 1);
    assert_eq!(pls[0]["itemCount"], 1);
}

#[tokio::test]
async fn m7_subsonic_api_compatibility() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let _admin_token = app.setup_admin().await;

    // 1. Subsonic Ping (JSON)
    let res = app.call(Method::GET, "/rest/ping?f=json", None, None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["subsonic-response"]["status"], "ok");
    assert_eq!(res.body["subsonic-response"]["openSubsonic"], true);

    // 2. Subsonic GetLicense
    let res =
        app.call(Method::GET, "/rest/getLicense?f=json&u=admin&p=admin-password", None, None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["subsonic-response"]["status"], "ok");

    // 3. Subsonic GetMusicFolders
    let res = app
        .call(Method::GET, "/rest/getMusicFolders?f=json&u=admin&p=admin-password", None, None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["subsonic-response"]["status"], "ok");

    // 4. Subsonic GetArtists
    let res =
        app.call(Method::GET, "/rest/getArtists?f=json&u=admin&p=admin-password", None, None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["subsonic-response"]["status"], "ok");
}

#[tokio::test]
async fn m7_photos_timeline_endpoint() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // Create Photo library
    let dir = app.media_dir("Photos");
    let res = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_token),
            Some(json!({ "name": "Vacation Photos", "kind": "photo", "paths": [dir] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let lib_id = res.body["id"].as_i64().unwrap();

    // Insert photo items with exif_date (e.g. 2024-07-15)
    let epoch_ms = 1721044800000_i64; // July 2024
    let _ = sqlx::query(
        "INSERT INTO media_items (library_id, kind, title, sort_title, exif_date, camera_make, camera_model, added_at, updated_at)
         VALUES (?, 'photo', 'Beach sunset.jpg', 'beach sunset.jpg', ?, 'Sony', 'A7 IV', 100, 100)",
    )
    .bind(lib_id)
    .bind(epoch_ms)
    .execute(&db.write)
    .await
    .unwrap();

    // Fetch photo timeline
    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/libraries/{lib_id}/timeline"),
            Some(&admin_token),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let buckets = res.body.as_array().unwrap();
    assert!(!buckets.is_empty());
    assert_eq!(buckets[0]["year"], 2024);
    assert_eq!(buckets[0]["month"], 7);
    assert_eq!(buckets[0]["count"], 1);
}

#[tokio::test]
async fn m7_media_requests_lifecycle() {
    let (app, _db, _) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Submit Media Request
    let res = app
        .call(
            Method::POST,
            "/api/v1/requests",
            Some(&admin_token),
            Some(json!({
                "kind": "movie",
                "tmdbId": 157336,
                "title": "Interstellar",
                "year": 2014
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let req_id = res.body["id"].as_i64().unwrap();
    assert_eq!(res.body["status"], "pending");

    // 2. List Requests
    let res = app.call(Method::GET, "/api/v1/requests", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let list = res.body.as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["title"], "Interstellar");

    // 3. Update Request Status (e.g. Radarr approved)
    let res = app
        .call(
            Method::PATCH,
            &format!("/api/v1/requests/{req_id}/status"),
            Some(&admin_token),
            Some(json!({
                "status": "approved",
                "arrId": 42
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    // 4. Hardlink Analysis endpoint
    let res =
        app.call(Method::GET, "/api/v1/requests/hardlink-analysis", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body["totalFileSizeBytes"].is_number());
}

#[tokio::test]
async fn m7_webhooks_management() {
    let (app, _db, _) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Create Webhook
    let res = app
        .call(
            Method::POST,
            "/api/v1/webhooks",
            Some(&admin_token),
            Some(json!({
                "name": "Home Assistant Automation",
                "url": "http://127.0.0.1:8123/api/webhook/mbl",
                "events": ["playback.started", "playback.stopped"],
                "secret": "webhook-secret-key"
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let hook_id = res.body["id"].as_i64().unwrap();
    assert_eq!(res.body["enabled"], true);

    // 2. List Webhooks
    let res = app.call(Method::GET, "/api/v1/webhooks", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let list = res.body.as_array().unwrap();
    assert_eq!(list.len(), 1);

    // 3. Toggle Webhook (disable)
    let res = app
        .call(
            Method::PATCH,
            &format!("/api/v1/webhooks/{hook_id}"),
            Some(&admin_token),
            Some(json!({ "enabled": false })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    // Verify it is disabled
    let res = app.call(Method::GET, "/api/v1/webhooks", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body[0]["enabled"], false);
}

#[tokio::test]
async fn m7_players_and_companion_interaction() {
    let (app, _db, services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Register a virtual player
    services.players.register_or_update(PlayerInfo {
        id: "living-room-tv".into(),
        name: "Living Room Apple TV".into(),
        device_type: "tv".into(),
        state: "idle".into(),
        now_playing_item_id: None,
        position_ms: 0,
        duration_ms: 0,
        volume: 75,
        last_seen_at: 0,
    });

    // 2. List Players
    let res = app.call(Method::GET, "/api/v1/players", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let players = res.body.as_array().unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(players[0]["name"], "Living Room Apple TV");

    let auth = services.auth.authenticate(&admin_token).await.unwrap();
    let mut connection =
        services.players.connect(&auth.actor, auth.session_id, "living-room-tv").unwrap();
    // 3. Send Player Command
    let res = app
        .call(
            Method::POST,
            "/api/v1/players/living-room-tv/command",
            Some(&admin_token),
            Some(json!({
                "action": "setVolume",
                "params": { "volume": 85 }
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    assert!(matches!(
        connection.commands.recv().await.unwrap(),
        mbl_service::PlayerCommand::SetVolume { volume: 85 }
    ));
    // 4. Companion co-watching join and reaction
    let fixture = common::seed_playback_fixtures(&app, &_db, &admin_token).await;
    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin_token),
            Some(json!({"itemId": fixture.movie_id, "profile": "chrome", "startMs": 42000})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK);
    let session_id = start.body["sessionId"].as_str().unwrap();
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/companion/sessions/{session_id}/join"),
            Some(&admin_token),
            Some(json!({ "botName": "Hermes" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/companion/sessions/{session_id}/reaction"),
            Some(&admin_token),
            Some(json!({
                "botName": "Hermes",
                "timestampMs": 42000,
                "commentary": "Watch out for the door!",
                "sentiment": "shocked"
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["commentary"], "Watch out for the door!");
}

#[tokio::test]
async fn m7_matchmaker_swipe_session() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // Create a movie item for voting
    let dir = app.media_dir("Movies");
    let res = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_token),
            Some(json!({ "name": "Movies", "kind": "movie", "paths": [dir] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let lib_id = res.body["id"].as_i64().unwrap();

    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'movie', 'Superbad', 'superbad', 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    // 1. Create Matchmaker Session
    let res = app
        .call(
            Method::POST,
            "/api/v1/matchmaker/sessions",
            Some(&admin_token),
            Some(json!({ "filterConfig": "{\"genre\":\"Comedy\"}" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let session_id = res.body["id"].as_str().unwrap();

    let bob = app.create_user(&admin_token, "bob").await;
    let join = app
        .call(
            Method::POST,
            &format!("/api/v1/matchmaker/sessions/{session_id}/join"),
            Some(&bob),
            None,
        )
        .await;
    assert_eq!(join.status, StatusCode::NO_CONTENT);
    // 2. Submit swipe votes from user 1
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/matchmaker/sessions/{session_id}/vote"),
            Some(&admin_token),
            Some(json!({
                "userName": "Alice",
                "itemId": item_id,
                "vote": 1
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["matched"], false);

    // 3. Submit swipe votes from user 2 (triggering match)
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/matchmaker/sessions/{session_id}/vote"),
            Some(&bob),
            Some(json!({
                "userName": "Bob",
                "itemId": item_id,
                "vote": 1
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["matched"], true);
    assert_eq!(res.body["matchedItemId"], item_id);
}

#[test]
fn m7_analysis_subtitles_auto_sync() {
    let speech = vec![
        mbl_analysis::SpeechSegment { start_ms: 3_000, end_ms: 5_000 },
        mbl_analysis::SpeechSegment { start_ms: 8_000, end_ms: 10_000 },
    ];
    let srt = "1\n00:00:05,800 --> 00:00:07,800\nHello world\n\n2\n00:00:10,800 --> 00:00:12,800\nWelcome to MBL\n";
    let (aligned_srt, offset) = align_srt_content(srt, &speech);
    assert!((-2900..=-2700).contains(&offset));
    assert!(aligned_srt.contains("Hello world"));
    assert!(aligned_srt.contains("Welcome to MBL"));
}

#[test]
fn m7_analysis_loudness_ebur128_parser() {
    let sample_output = r#"
    [Parsed_ebur128_0 @ 0x55aa] Summary:
      Integrated loudness:
        I:         -14.2 LUFS
        Threshold: -24.3 LUFS

      Loudness range:
        LRA:         7.8 LU
        Threshold: -34.5 LUFS
        LRA low:   -19.1 LUFS
        LRA high:  -11.3 LUFS

      True peak:
        Peak:       -1.1 dBFS
    "#;
    let res = parse_ebur128_output(sample_output).unwrap();
    assert_eq!(res.integrated_lufs, -14.2);
    assert_eq!(res.loudness_range, 7.8);
    assert_eq!(res.true_peak, -1.1);
}

#[test]
fn m7_spoiler_free_recap_strict_boundary() {
    use mbl_core::ItemId;
    use std::collections::HashSet;

    let episodes = vec![
        mbl_analysis::features::EpisodeContext {
            episode_id: ItemId(1),
            season_number: 1,
            episode_number: 1,
            title: "Pilot".into(),
            summary: Some("The journey begins as our heroes meet.".into()),
            transcript_cue_texts: vec![],
        },
        mbl_analysis::features::EpisodeContext {
            episode_id: ItemId(2),
            season_number: 1,
            episode_number: 2,
            title: "Forest".into(),
            summary: Some("A betrayal shakes the group in the forest.".into()),
            transcript_cue_texts: vec![],
        },
        mbl_analysis::features::EpisodeContext {
            episode_id: ItemId(3),
            season_number: 1,
            episode_number: 3,
            title: "Siege".into(),
            summary: Some("The fortress falls in a surprise siege.".into()),
            transcript_cue_texts: vec![],
        },
        mbl_analysis::features::EpisodeContext {
            episode_id: ItemId(4),
            season_number: 1,
            episode_number: 4,
            title: "Coronation".into(),
            summary: Some("MAJOR SPOILER: The king dies in episode 4.".into()),
            transcript_cue_texts: vec![],
        },
    ];

    let mut watched = HashSet::new();
    watched.insert(ItemId(1));
    watched.insert(ItemId(2));

    let prompt = build_spoiler_free_recap("Epic Show", ItemId(3), &episodes, &watched);
    assert_eq!(prompt.eligible_context_episodes.len(), 2);
    assert!(prompt.prompt_text.contains("Pilot"));
    assert!(prompt.prompt_text.contains("Forest"));
    assert!(!prompt.prompt_text.contains("Siege"));
    assert!(!prompt.prompt_text.contains("Coronation"));
    assert!(!prompt.prompt_text.contains("king dies"));
}

#[test]
fn m7_remote_mqtt_and_dlna_discovery() {
    // 1. DLNA UPnP root device description XML generator
    let xml = generate_device_description("http://192.168.1.50:8420", "MBL Server", "12345678");
    assert!(xml.contains("urn:schemas-upnp-org:device:MediaServer:1"));
    assert!(xml.contains("MBL Server"));
    assert!(xml.contains("urn:schemas-upnp-org:service:ContentDirectory:1"));

    // 2. Home Assistant MQTT Discovery payloads
    let (topic, json) = build_player_discovery("mbl", "living_room_tv", "Living Room TV", "0.6.0");
    assert_eq!(topic, "homeassistant/media_player/mbl_living_room_tv/config");
    assert!(json.contains("Living Room TV"));
    assert!(json.contains("mbl/player/living_room_tv/state"));
    assert!(json.contains("mbl/player/living_room_tv/command"));

    let cmd = parse_incoming_command("PLAY");
    assert_eq!(cmd, IncomingPlayerCommand::Play);
}
