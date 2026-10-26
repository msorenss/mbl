//! End-to-end integration tests for Milestone M5:
//! - M5.1 Thermal and power monitoring + JobRunner pausing Priority::Idle above 75 °C
//! - M5.2 Platform profile defaults & M5.3 tmpfs transcode directory selection
//! - M5.4 Pre-transcoded optimized versions (`source = 'optimized'`) automatically direct-playing
//!   when primary version requires transcoding
//! - M5.5 Trickplay scrub thumbnails (`index.json` + `sprite_000.jpg`)
//! - M5.6 & M5.7 Intro and credits markers merged into `PlaybackResponse`
//! - M5.8 Quick-Connect 6-digit TV pairing flow
//! - M5.13 Hailo-10H AI Media Intelligence (`index-scenes`, `search_scenes`, `get_spoiler_free_recap`)
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use mbl_core::ItemId;
use mbl_core::events::EventBus;
use mbl_core::time::now_ms;
use mbl_db::repo::m5::MarkersRepo;
use mbl_jobs::{ActivityProbe, JobCtx, JobHandler, JobRunner, Priority, RunnerConfig};
use mbl_mcp::McpClient;
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct CountingIdleHandler {
    runs: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl JobHandler for CountingIdleHandler {
    fn kind(&self) -> &'static str {
        "m5_idle_test"
    }

    async fn run(&self, _ctx: JobCtx, _payload: serde_json::Value) -> mbl_core::Result<()> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn m5_1_thermal_health_and_idle_job_pausing_above_75c() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin_session = app.setup_admin().await;

    // 1. Verify GET /api/v1/system/health returns thermal and defaults
    let health_res = app.call(Method::GET, "/api/v1/system/health", None, None).await;
    assert_eq!(health_res.status, StatusCode::OK);
    assert!(health_res.body["status"].is_string());
    assert!(health_res.body["thermal"].is_object());
    assert!(health_res.body["defaults"]["probeConcurrency"].is_number());

    // 2. Verify JobRunner pauses Priority::Idle when sysfs fixture reports > 75 °C
    let sysfs = tempfile::tempdir().unwrap();
    let tz_dir = sysfs.path().join("sys/class/thermal/thermal_zone0");
    std::fs::create_dir_all(&tz_dir).unwrap();
    // Write 78.0 °C (78000 millidegrees)
    std::fs::write(tz_dir.join("temp"), "78000\n").unwrap();

    let playback_probe =
        Arc::new(services.playback.clone().with_thermal_root(sysfs.path().to_path_buf()));
    assert!(playback_probe.thermal_throttled());

    let mut runner = JobRunner::new(
        db.clone(),
        EventBus::new(128),
        RunnerConfig { poll_interval: Duration::from_millis(80), ..RunnerConfig::default() },
    );
    let runs = Arc::new(AtomicUsize::new(0));
    runner.register(Arc::new(CountingIdleHandler { runs: runs.clone() }));
    runner.set_activity_probe(playback_probe);

    let queue = runner.queue();
    queue.enqueue("m5_idle_test", &json!({}), Priority::Idle).await.unwrap();

    let shutdown = CancellationToken::new();
    let handle = runner.start(shutdown.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    // While at 78 °C, the Idle job must remain paused (0 runs)
    assert_eq!(runs.load(Ordering::SeqCst), 0);

    // Cool down sysfs fixture to 52.0 °C (52000 millidegrees)
    std::fs::write(tz_dir.join("temp"), "52000\n").unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    shutdown.cancel();
    let _ = handle.await;

    // Also check MCP get_health includes thermal report
    let tok_res = app
        .call(
            Method::POST,
            "/api/v1/auth/tokens",
            Some(&admin_session),
            Some(json!({ "name": "m5-health", "scopes": ["read"] })),
        )
        .await;
    let token = tok_res.body["token"].as_str().unwrap().to_owned();
    let mut mcp = McpClient::connect_router(app.router.clone(), Some(token));
    let _ = mcp.initialize().await.unwrap();
    let mcp_health = mcp.call_tool("get_health", json!({})).await.unwrap();
    assert!(!mcp_health.is_error);
    assert!(mcp_health.structured_content.unwrap()["thermal"].is_object());
}

#[tokio::test]
async fn m5_4_optimized_version_e2e_direct_plays_on_h264_only_client() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin_session = app.setup_admin().await;
    let s = seed_playback_fixtures(&app, &db, &admin_session).await;

    // `s.ep1_id` is seeded as 4K HEVC Main10 + AC3 in MKV, so Firefox cannot DirectPlay it.

    // 1. Before optimization: ep1_id is 4K HEVC Main10 + AC3 in MKV -> Firefox cannot direct-play it (must transcode).
    let pre_play = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin_session),
            Some(json!({
                "itemId": s.ep1_id,
                "profile": "firefox"
            })),
        )
        .await;
    assert_eq!(pre_play.status, StatusCode::OK, "pre_play failed: {:?}", pre_play.body);
    assert_eq!(pre_play.body["method"], "transcode");
    if let Some(sid) = pre_play.body["sessionId"].as_str() {
        let _ = app
            .call(Method::POST, &format!("/api/v1/playback/{sid}/stop"), Some(&admin_session), None)
            .await;
    }

    // 2. Preview optimization via dry_run: true
    let dry_res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/optimize", s.ep1_id),
            Some(&admin_session),
            Some(json!({ "profile": "universal_1080p", "dryRun": true })),
        )
        .await;
    assert_eq!(dry_res.status, StatusCode::OK, "dry_run optimize failed: {:?}", dry_res.body);
    assert_eq!(dry_res.body["dryRun"], true);
    assert!(dry_res.body["estimate"]["estimatedSizeBytes"].as_u64().unwrap() > 0);

    // 3. Create an optimized version via REST POST /api/v1/items/{id}/optimize
    let opt_res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/optimize", s.ep1_id),
            Some(&admin_session),
            Some(json!({ "profile": "universal_1080p", "dryRun": false })),
        )
        .await;
    assert_eq!(opt_res.status, StatusCode::OK, "optimize failed: {:?}", opt_res.body);
    assert_eq!(opt_res.body["dryRun"], false);
    let opt_part_id = opt_res.body["partId"].as_i64().unwrap();

    // 4. Request playback again on Firefox -> must DirectPlay the optimized H.264+AAC MP4 version!
    let post_play = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin_session),
            Some(json!({
                "itemId": s.ep1_id,
                "profile": "firefox"
            })),
        )
        .await;
    assert_eq!(post_play.status, StatusCode::OK, "post_play failed: {:?}", post_play.body);
    assert_eq!(post_play.body["method"], "directPlay");
    let url = post_play.body["url"].as_str().unwrap();
    assert!(
        url.starts_with(&format!("/api/v1/stream/direct/{opt_part_id}?token=")),
        "unexpected direct URL: {url}"
    );
    if let Some(sid) = post_play.body["sessionId"].as_str() {
        let _ = app
            .call(Method::POST, &format!("/api/v1/playback/{sid}/stop"), Some(&admin_session), None)
            .await;
    }
}

#[tokio::test]
async fn m5_5_6_7_8_13_trickplay_markers_quick_connect_and_hailo_intelligence() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin_session = app.setup_admin().await;
    let s = seed_playback_fixtures(&app, &db, &admin_session).await;

    // --- M5.5 Trickplay manifest & sprite ---
    let part_id: i64 = sqlx::query_scalar(
        "SELECT mp.id FROM media_parts mp
         JOIN media_versions mv ON mv.id = mp.version_id
         WHERE mv.item_id = ? LIMIT 1",
    )
    .bind(s.movie_id)
    .fetch_one(&db.read)
    .await
    .unwrap();

    let tp_index = app
        .call(
            Method::GET,
            &format!("/api/v1/stream/trickplay/{part_id}/index.json"),
            Some(&admin_session),
            None,
        )
        .await;
    assert_eq!(tp_index.status, StatusCode::OK);
    assert_eq!(tp_index.body["tileWidth"], 320);
    assert_eq!(tp_index.body["tileHeight"], 180);
    assert_eq!(tp_index.body["intervalMs"], 10000);
    assert!(!tp_index.body["sprites"].as_array().unwrap().is_empty());

    let tp_sprite = app
        .call(
            Method::GET,
            &format!("/api/v1/stream/trickplay/{part_id}/sprite_000.jpg"),
            Some(&admin_session),
            None,
        )
        .await;
    assert_eq!(tp_sprite.status, StatusCode::OK);

    // --- M5.6 & M5.7 Intro and Credits markers in PlaybackResponse ---
    let markers_repo = MarkersRepo::new(db.clone());
    markers_repo
        .upsert(
            ItemId(s.movie_id),
            None,
            "intro",
            Some("Intro"),
            5_000,
            35_000,
            "fingerprint",
            0.92,
            now_ms(),
        )
        .await
        .unwrap();
    markers_repo
        .upsert(
            ItemId(s.movie_id),
            None,
            "credits",
            Some("End Credits"),
            110_000,
            120_000,
            "silence_black",
            0.88,
            now_ms(),
        )
        .await
        .unwrap();

    let play_res = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin_session),
            Some(json!({ "itemId": s.movie_id, "profile": "chrome" })),
        )
        .await;
    assert_eq!(play_res.status, StatusCode::OK);
    let markers = play_res.body["markers"].as_array().unwrap();
    assert!(markers.iter().any(|m| m["kind"] == "intro" && m["startMs"] == 5_000));
    assert!(markers.iter().any(|m| m["kind"] == "credits" && m["startMs"] == 110_000));

    // --- M5.8 Quick-Connect 6-digit TV pairing flow ---
    let qc_start = app
        .call(
            Method::POST,
            "/api/v1/auth/quick-connect",
            None,
            Some(json!({ "deviceName": "Living Room Pi TV", "clientName": "mbl-tv" })),
        )
        .await;
    assert_eq!(qc_start.status, StatusCode::OK);
    let code = qc_start.body["code"].as_str().unwrap().to_owned();
    let secret = qc_start.body["secret"].as_str().unwrap().to_owned();
    assert_eq!(code.len(), 6);

    let qc_poll1 = app
        .call(
            Method::POST,
            "/api/v1/auth/quick-connect/poll",
            None,
            Some(json!({ "secret": secret })),
        )
        .await;
    assert_eq!(qc_poll1.status, StatusCode::OK);
    assert_eq!(qc_poll1.body["authorized"], false);

    let qc_auth = app
        .call(
            Method::POST,
            "/api/v1/auth/quick-connect/authorize",
            Some(&admin_session),
            Some(json!({ "code": code })),
        )
        .await;
    assert_eq!(qc_auth.status, StatusCode::NO_CONTENT);

    let qc_poll2 = app
        .call(
            Method::POST,
            "/api/v1/auth/quick-connect/poll",
            None,
            Some(json!({ "secret": secret })),
        )
        .await;
    assert_eq!(qc_poll2.status, StatusCode::OK);
    assert_eq!(qc_poll2.body["authorized"], true);
    assert!(qc_poll2.body["token"].as_str().unwrap().len() > 10);

    // --- M5.13 Hailo-10H AI Scene Search & Spoiler-Free Recap (REST + MCP) ---
    let idx_res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/index-scenes", s.movie_id),
            Some(&admin_session),
            Some(json!({
                "scenes": [
                    { "timestampMs": 1000, "description": "car chase in the snow" }
                ]
            })),
        )
        .await;
    assert_eq!(idx_res.status, StatusCode::OK);
    assert!(!idx_res.body.as_array().unwrap().is_empty());

    let search_res = app
        .call(
            Method::GET,
            "/api/v1/ai/search-scenes?q=car+chase+in+the+snow&limit=5",
            Some(&admin_session),
            None,
        )
        .await;
    assert_eq!(search_res.status, StatusCode::OK);
    let hits = search_res.body.as_array().unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0]["itemId"], s.movie_id);

    // Mark ep1 watched, leave ep2 unwatched, and verify spoiler-free recap!
    sqlx::query("UPDATE media_items SET summary = 'Episode 1: Walter starts cooking in the RV.' WHERE id = ?")
        .bind(s.ep1_id)
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE media_items SET summary = 'Episode 2 SECRET SPOILER: Tuco discovers the truth.' WHERE id = ?")
        .bind(s.ep2_id)
        .execute(&db.write)
        .await
        .unwrap();

    app.call(
        Method::POST,
        &format!("/api/v1/items/{}/watched", s.ep1_id),
        Some(&admin_session),
        None,
    )
    .await;

    let recap_res = app
        .call(Method::GET, &format!("/api/v1/ai/recap/{}", s.show_id), Some(&admin_session), None)
        .await;
    assert_eq!(recap_res.status, StatusCode::OK);
    assert_eq!(recap_res.body["episodesIncluded"], 1);
    let recap_text = recap_res.body["recapText"].as_str().unwrap();
    assert!(recap_text.contains("Episode 1"));
    assert!(!recap_text.contains("SECRET SPOILER"));

    // Verify M5 MCP tools: search_scenes, get_spoiler_free_recap, create_optimized_versions
    let tok_res = app
        .call(
            Method::POST,
            "/api/v1/auth/tokens",
            Some(&admin_session),
            Some(json!({ "name": "m5-ai-agent", "scopes": ["admin"] })),
        )
        .await;
    let token = tok_res.body["token"].as_str().unwrap().to_owned();
    let mut mcp = McpClient::connect_router(app.router.clone(), Some(token));
    let _ = mcp.initialize().await.unwrap();

    let mcp_scenes = mcp
        .call_tool("search_scenes", json!({ "query": "car chase in the snow", "limit": 5 }))
        .await
        .unwrap();
    assert!(!mcp_scenes.is_error);
    assert!(mcp_scenes.structured_content.unwrap()["count"].as_u64().unwrap() >= 1);

    let mcp_recap =
        mcp.call_tool("get_spoiler_free_recap", json!({ "item_id": s.show_id })).await.unwrap();
    assert!(!mcp_recap.is_error);
    assert_eq!(mcp_recap.structured_content.unwrap()["episodesIncluded"], 1);

    let mcp_opt_dry = mcp
        .call_tool(
            "create_optimized_versions",
            json!({
                "item_id": s.movie_id,
                "profile": "universal_720p",
                "dry_run": true
            }),
        )
        .await
        .unwrap();
    assert!(!mcp_opt_dry.is_error);
    assert_eq!(mcp_opt_dry.structured_content.unwrap()["dryRun"], true);
}
