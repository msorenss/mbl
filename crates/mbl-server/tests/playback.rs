//! Integration tests for M2.7 Playback API and watch state:
//! - Decision -> URL flow (DirectPlay + HLS Transcode, subtitle tracks, markers)
//! - Progress -> Resume offset (1 min < pos < 90%) and Continue Watching hub
//! - Watched thresholds (>= 90% and < 3 min remaining) and `watch_history` on stop
//! - `POST/DELETE /api/v1/items/{id}/watched` with show/season cascade
//! - `ActivityProbe` hook pausing idle jobs during active playback

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use mbl_jobs::ActivityProbe;
use serde_json::json;

#[tokio::test]
async fn playback_decision_to_url_flow_direct_and_hls_transcode() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let s = seed_playback_fixtures(&app, &db, &admin).await;

    assert!(!services.playback.playback_active());

    // 1. Direct play (H.264/AAC MP4 on Chrome)
    let res = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({
                "itemId": s.movie_id,
                "profile": "chrome"
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["method"], "directPlay");
    assert_eq!(res.body["reasons"], json!([]));
    assert_eq!(res.body["startMs"], 0);
    assert_eq!(res.body["durationMs"], 6_000_000);

    let url = res.body["url"].as_str().unwrap();
    assert!(
        url.starts_with(&format!("/api/v1/stream/direct/{}?token=", s.movie_part_id)),
        "unexpected direct URL: {url}"
    );

    let subs = res.body["subtitles"].as_array().unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0]["index"], 2);
    assert_eq!(subs[0]["format"], "vtt");
    assert!(
        subs[0]["url"]
            .as_str()
            .unwrap()
            .starts_with(&format!("/api/v1/stream/subtitles/{}/2.vtt?token=", s.movie_part_id))
    );

    let markers = res.body["markers"].as_array().unwrap();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0]["kind"], "intro");
    assert_eq!(markers[1]["kind"], "credits");

    // ActivityProbe must report playback active while session is running
    assert!(services.playback.playback_active());

    let direct_sid = res.body["sessionId"].as_str().unwrap().to_owned();
    let stop_res = app
        .call(Method::POST, &format!("/api/v1/playback/{direct_sid}/stop"), Some(&admin), None)
        .await;
    assert_eq!(stop_res.status, StatusCode::NO_CONTENT);
    assert!(!services.playback.playback_active());

    // 2. Transcode (4K HEVC Main10 + AC3 in MKV on Firefox) -> HLS URL
    let res = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({
                "itemId": s.ep1_id,
                "partId": s.ep1_part_id,
                "profile": "firefox"
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["method"], "transcode");
    let reasons = res.body["reasons"].as_array().unwrap();
    assert!(!reasons.is_empty(), "transcode must include human-readable reasons");

    let hls_sid = res.body["sessionId"].as_str().unwrap();
    let hls_url = res.body["url"].as_str().unwrap();
    assert_eq!(hls_url, format!("/api/v1/stream/hls/{hls_sid}/master.m3u8"));

    // Verify upfront master playlist is immediately served at the returned URL
    let m3u8_res = app.call(Method::GET, hls_url, None, None).await;
    assert_eq!(m3u8_res.status, StatusCode::OK);
    assert!(m3u8_res.body.as_str().unwrap().contains("#EXTM3U"));

    let _ = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{hls_sid}/stop"),
            Some(&admin),
            Some(json!({ "positionMs": 5_000 })),
        )
        .await;
}

#[tokio::test]
async fn playback_progress_resume_and_watched_thresholds_and_cascade() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let s = seed_playback_fixtures(&app, &db, &admin).await;

    // Start playback of the 100-min movie (6_000_000 ms)
    let start = app
        .call(Method::POST, "/api/v1/playback", Some(&admin), Some(json!({ "itemId": s.movie_id })))
        .await;
    assert_eq!(start.status, StatusCode::OK);
    let sid = start.body["sessionId"].as_str().unwrap().to_owned();

    // 1. Progress at 30 s (<= 1 min) -> no resume offset stored
    let p1 = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{sid}/progress"),
            Some(&admin),
            Some(json!({ "positionMs": 30_000, "state": "playing" })),
        )
        .await;
    assert_eq!(p1.status, StatusCode::NO_CONTENT);
    let detail =
        app.call(Method::GET, &format!("/api/v1/items/{}", s.movie_id), Some(&admin), None).await;
    assert_eq!(detail.body["watch"]["viewOffsetMs"], 0);
    assert_eq!(detail.body["watch"]["viewCount"], 0);

    // 2. Progress at 15 min (900_000 ms, 1 min < pos < 90%) -> resume offset stored
    let p2 = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{sid}/progress"),
            Some(&admin),
            Some(json!({ "positionMs": 900_000, "state": "playing" })),
        )
        .await;
    assert_eq!(p2.status, StatusCode::NO_CONTENT);
    let detail =
        app.call(Method::GET, &format!("/api/v1/items/{}", s.movie_id), Some(&admin), None).await;
    assert_eq!(detail.body["watch"]["viewOffsetMs"], 900_000);
    assert_eq!(detail.body["watch"]["viewCount"], 0);

    // Stop session -> records watch_history and preserves resume offset
    let stop = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{sid}/stop"),
            Some(&admin),
            Some(json!({ "positionMs": 900_000 })),
        )
        .await;
    assert_eq!(stop.status, StatusCode::NO_CONTENT);

    let history_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM watch_history WHERE item_id = ?")
            .bind(s.movie_id)
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert_eq!(history_count, 1);

    // 3. Starting a new playback session without `startMs` resumes at 900_000!
    let resume = app
        .call(Method::POST, "/api/v1/playback", Some(&admin), Some(json!({ "itemId": s.movie_id })))
        .await;
    assert_eq!(resume.status, StatusCode::OK);
    assert_eq!(resume.body["startMs"], 900_000);
    let sid2 = resume.body["sessionId"].as_str().unwrap().to_owned();

    // 4. Progress at 92% (5_520_000 ms >= 90%) -> marks watched and clears resume offset
    let p3 = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{sid2}/progress"),
            Some(&admin),
            Some(json!({ "positionMs": 5_520_000, "state": "playing" })),
        )
        .await;
    assert_eq!(p3.status, StatusCode::NO_CONTENT);
    let detail =
        app.call(Method::GET, &format!("/api/v1/items/{}", s.movie_id), Some(&admin), None).await;
    assert_eq!(detail.body["watch"]["viewOffsetMs"], 0);
    assert_eq!(detail.body["watch"]["viewCount"], 1);

    let _ =
        app.call(Method::POST, &format!("/api/v1/playback/{sid2}/stop"), Some(&admin), None).await;

    // 5. Fewer than 3 minutes remaining threshold on 25-min episode (1_500_000 ms):
    // At 22m10s (1_330_000 ms = 88.6% < 90%, 2m50s remaining < 3 min) -> marks watched!
    let ep_start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({ "itemId": s.ep1_id, "profile": "chrome" })),
        )
        .await;
    let ep_sid = ep_start.body["sessionId"].as_str().unwrap().to_owned();
    let ep_prog = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{ep_sid}/progress"),
            Some(&admin),
            Some(json!({ "positionMs": 1_330_000, "state": "playing" })),
        )
        .await;
    assert_eq!(ep_prog.status, StatusCode::NO_CONTENT);
    let ep_detail =
        app.call(Method::GET, &format!("/api/v1/items/{}", s.ep1_id), Some(&admin), None).await;
    assert_eq!(ep_detail.body["watch"]["viewCount"], 1);
    assert_eq!(ep_detail.body["watch"]["viewOffsetMs"], 0);
    let _ = app
        .call(Method::POST, &format!("/api/v1/playback/{ep_sid}/stop"), Some(&admin), None)
        .await;

    // 6. Cascade POST /api/v1/items/{id}/watched and DELETE /api/v1/items/{id}/watched on Show & Season
    let w_show = app
        .call(Method::POST, &format!("/api/v1/items/{}/watched", s.show_id), Some(&admin), None)
        .await;
    assert_eq!(w_show.status, StatusCode::NO_CONTENT);

    for id in [s.show_id, s.season_id, s.ep1_id, s.ep2_id] {
        let d = app.call(Method::GET, &format!("/api/v1/items/{id}"), Some(&admin), None).await;
        assert_eq!(d.body["watch"]["viewCount"], 1, "item {id} should be watched");
    }

    let unw_season = app
        .call(Method::DELETE, &format!("/api/v1/items/{}/watched", s.season_id), Some(&admin), None)
        .await;
    assert_eq!(unw_season.status, StatusCode::NO_CONTENT);

    for id in [s.season_id, s.ep1_id, s.ep2_id] {
        let d = app.call(Method::GET, &format!("/api/v1/items/{id}"), Some(&admin), None).await;
        assert_eq!(d.body["watch"]["viewCount"], 0, "item {id} should be unwatched");
    }
}
