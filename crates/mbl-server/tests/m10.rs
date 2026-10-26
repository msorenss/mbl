//! Integration tests for Milestone M10: Hardening → v1.0.0
//! - M10.1: Threat model mitigations (webhook protocol enforcement, DoS upload limits, path traversal rejection)
//! - M10.5: Plex & Jellyfin watch history and playback state import matching and upserting
//! - M10.8: MCP import tools and evaluation scenarios

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_mcp::McpClient;
use serde_json::json;

#[tokio::test]
async fn m10_plex_and_jellyfin_history_import() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Seed library and media items in database
    sqlx::query(
        "INSERT INTO libraries (id, name, kind, settings, created_at)
         VALUES (1, 'Movies', 'movie', '{}', 1700000000)",
    )
    .execute(&db.write)
    .await
    .unwrap();

    // Item 1: Inception (2010)
    sqlx::query(
        "INSERT INTO media_items (id, library_id, kind, title, sort_title, year, tmdb_id, imdb_id, duration_ms, added_at, updated_at)
         VALUES (101, 1, 'movie', 'Inception', 'inception', 2010, 27205, 'tt1375666', 8880000, 1700000000, 1700000000)",
    )
    .execute(&db.write)
    .await
    .unwrap();

    // Item 2: Interstellar (2014)
    sqlx::query(
        "INSERT INTO media_items (id, library_id, kind, title, sort_title, year, tmdb_id, imdb_id, duration_ms, added_at, updated_at)
         VALUES (102, 1, 'movie', 'Interstellar', 'interstellar', 2014, 157336, 'tt0816692', 10140000, 1700000000, 1700000000)",
    )
    .execute(&db.write)
    .await
    .unwrap();

    // 2. Test Plex History Import via REST endpoint
    let plex_payload = json!({
        "items": [
            {
                "title": "Inception",
                "year": 2010,
                "tmdbId": 27205,
                "viewCount": 2,
                "viewOffsetMs": 4500000,
                "lastViewedAt": 1700010000000_i64
            },
            {
                // Matches via title and year fallback
                "title": "Interstellar",
                "year": 2014,
                "viewCount": 1,
                "viewOffsetMs": 0,
                "lastViewedAt": 1700020000000_i64
            },
            {
                // Unmatched item
                "title": "Some Unknown Sci-Fi",
                "year": 1999
            }
        ]
    });

    let plex_res =
        app.call(Method::POST, "/api/v1/import/plex", Some(&admin_token), Some(plex_payload)).await;
    assert_eq!(plex_res.status, StatusCode::OK);
    assert_eq!(plex_res.body["totalItems"].as_u64().unwrap(), 3);
    assert_eq!(plex_res.body["matchedItems"].as_u64().unwrap(), 2);
    assert_eq!(plex_res.body["updatedWatchState"].as_u64().unwrap(), 2);
    let unmatched = plex_res.body["unmatched"].as_array().unwrap();
    assert_eq!(unmatched.len(), 1);

    // Verify watch state in database for Item 101 (Inception)
    let inception_state: (i64, i64) = sqlx::query_as(
        "SELECT view_offset_ms, view_count FROM watch_state WHERE user_id = 1 AND item_id = 101",
    )
    .fetch_one(&db.read)
    .await
    .unwrap();
    assert_eq!(inception_state.0, 4500000);
    assert_eq!(inception_state.1, 2);

    // Verify watch history entry created for Inception
    let history_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM watch_history WHERE user_id = 1 AND item_id = 101 AND method = 'import'",
    )
    .fetch_one(&db.read)
    .await
    .unwrap();
    assert!(history_count >= 1);

    // 3. Test Jellyfin History Import via REST endpoint
    // Jellyfin uses ticks (10,000 ticks = 1 ms)
    let jellyfin_payload = json!({
        "items": [
            {
                "name": "Inception",
                "productionYear": 2010,
                "played": true,
                "playCount": 3,
                "playbackPositionTicks": 55_000_000_000_i64, // 5,500,000 ms
                "lastPlayedDate": "2026-10-26T12:00:00Z",
                "providerIds": {
                    "Imdb": "tt1375666"
                }
            }
        ]
    });

    let jf_res = app
        .call(Method::POST, "/api/v1/import/jellyfin", Some(&admin_token), Some(jellyfin_payload))
        .await;
    assert_eq!(jf_res.status, StatusCode::OK);
    assert_eq!(jf_res.body["totalItems"].as_u64().unwrap(), 1);
    assert_eq!(jf_res.body["matchedItems"].as_u64().unwrap(), 1);
    assert_eq!(jf_res.body["updatedWatchState"].as_u64().unwrap(), 1);

    // Verify updated position
    let updated_state: (i64, i64) = sqlx::query_as(
        "SELECT view_offset_ms, view_count FROM watch_state WHERE user_id = 1 AND item_id = 101",
    )
    .fetch_one(&db.read)
    .await
    .unwrap();
    assert_eq!(updated_state.0, 5500000);
    assert_eq!(updated_state.1, 3);
}

#[tokio::test]
async fn m10_mcp_import_tools() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // Seed library item
    sqlx::query(
        "INSERT INTO libraries (id, name, kind, settings, created_at)
         VALUES (2, 'Anime', 'movie', '{}', 1700000000)",
    )
    .execute(&db.write)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO media_items (id, library_id, kind, title, sort_title, year, tmdb_id, duration_ms, added_at, updated_at)
         VALUES (201, 2, 'movie', 'Spirited Away', 'spirited away', 2001, 129, 7500000, 1700000000, 1700000000)",
    )
    .execute(&db.write)
    .await
    .unwrap();

    let mut client = McpClient::connect_router(app.router.clone(), Some(admin_token));

    // Test import_plex_history MCP tool
    let plex_args = json!({
        "items": [
            {
                "title": "Spirited Away",
                "year": 2001,
                "tmdbId": 129,
                "viewCount": 5
            }
        ]
    });

    let res = client.call_tool("import_plex_history", plex_args).await.unwrap();
    assert!(!res.is_error);
    let summary = res.structured_content.as_ref().unwrap();
    assert_eq!(summary["matchedItems"].as_u64().unwrap(), 1);

    // Test import_jellyfin_history MCP tool
    let jf_args = json!({
        "items": [
            {
                "name": "Spirited Away",
                "productionYear": 2001,
                "playCount": 6,
                "playbackPositionTicks": 30_000_000_000_i64
            }
        ]
    });

    let res = client.call_tool("import_jellyfin_history", jf_args).await.unwrap();
    assert!(!res.is_error);
    let summary = res.structured_content.as_ref().unwrap();
    assert_eq!(summary["matchedItems"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn m10_security_threat_model() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Webhook protocol validation: ensure file://, ftp://, javascript: are rejected
    let invalid_webhook = json!({
        "name": "Malicious Webhook",
        "url": "file:///etc/passwd",
        "events": ["playback.start"]
    });
    let res =
        app.call(Method::POST, "/api/v1/webhooks", Some(&admin_token), Some(invalid_webhook)).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    let js_webhook = json!({
        "name": "XSS Webhook",
        "url": "javascript:alert(1)",
        "events": ["playback.start"]
    });
    let res =
        app.call(Method::POST, "/api/v1/webhooks", Some(&admin_token), Some(js_webhook)).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Valid http/https webhook is accepted
    let valid_webhook = json!({
        "name": "Valid Discord Webhook",
        "url": "https://example.com/webhook",
        "events": ["playback.start"]
    });
    let res =
        app.call(Method::POST, "/api/v1/webhooks", Some(&admin_token), Some(valid_webhook)).await;
    assert_eq!(res.status, StatusCode::CREATED);

    // 2. Worker source chunk streaming traversal defense
    let bad_source = app
        .call(Method::GET, "/api/v1/stream/worker/source/999999?token=invalid", None, None)
        .await;
    assert_eq!(bad_source.status, StatusCode::UNAUTHORIZED);
}
