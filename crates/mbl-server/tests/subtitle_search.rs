//! Subtitle search and download against a fake OpenSubtitles: the file lands next to the video
//! and becomes a track at once.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::Query;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::{get, post};
use common::{TestApp, seed_playback_fixtures};
use serde_json::{Value, json};
use std::collections::HashMap;

const SRT: &str = "1\n00:00:01,000 --> 00:00:03,000\nHej där\n";

/// Records search parameters and download tokens.
async fn fake_opensubtitles() -> (String, Arc<Mutex<Vec<String>>>) {
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let (s1, s2) = (seen.clone(), seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let link = format!("{base}/file.srt");
    let router = Router::new()
        .route(
            "/subtitles",
            get(move |h: HeaderMap, Query(q): Query<HashMap<String, String>>| {
                let seen = s1.clone();
                async move {
                    assert_eq!(h.get("api-key").unwrap(), "key");
                    let mut keys: Vec<String> = q.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    keys.sort();
                    seen.lock().unwrap().push(keys.join("&"));
                    axum::Json(json!({ "data": [
                        { "id": "1", "attributes": { "language": "sv", "release": "Heat.1995.BluRay",
                          "download_count": 10, "ratings": 8.0, "hearing_impaired": false,
                          "files": [{ "file_id": 7, "file_name": "heat.srt" }] } },
                        { "id": "2", "attributes": { "language": "sv", "release": "Heat.WEB",
                          "download_count": 99, "files": [{ "file_id": 8, "file_name": "heat2.srt" }] } }
                    ]}))
                }
            }),
        )
        .route(
            "/login",
            post(|axum::Json(b): axum::Json<Value>| async move {
                assert_eq!(b["username"], "anna");
                axum::Json(json!({ "token": "jwt" }))
            }),
        )
        .route(
            "/download",
            post(move |h: HeaderMap, axum::Json(b): axum::Json<Value>| {
                let seen = s2.clone();
                let link = link.clone();
                async move {
                    let auth = h.get("authorization").map(|v| v.to_str().unwrap().to_owned());
                    seen.lock().unwrap().push(format!("download {} {:?}", b["file_id"], auth));
                    axum::Json(json!({ "link": link, "file_name": "heat.srt", "remaining": 4 }))
                }
            }),
        )
        .route("/file.srt", get(|| async { SRT }));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (base, seen)
}

#[tokio::test]
async fn a_downloaded_subtitle_becomes_a_track() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    sqlx::query("UPDATE media_items SET tmdb_id = 949 WHERE id = ?")
        .bind(seeded.movie_id)
        .execute(&db.write)
        .await
        .unwrap();
    let search = format!("/api/v1/items/{}/subtitles/search?language=sv", seeded.movie_id);

    // Not set up yet.
    let res = app.call(Method::GET, "/api/v1/subtitles/status", Some(&viewer), None).await;
    assert_eq!(res.body["available"], false);
    let res = app.call(Method::GET, &search, Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);

    let (base, seen) = fake_opensubtitles().await;
    mbl_db::repo::settings::SettingsRepo::new(db.clone())
        .set("opensubtitles_base_url", &base)
        .await
        .unwrap();
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/opensubtitles",
            Some(&admin),
            Some(json!({ "apiKey": "key", "username": "anna", "password": "secret" })),
        )
        .await;
    assert_eq!(res.body, json!({ "hasApiKey": true, "username": "anna", "hasPassword": true }));
    let res = app.call(Method::GET, "/api/v1/system/opensubtitles", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app.call(Method::POST, "/api/v1/system/opensubtitles/test", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);

    // Search: by TMDB ID, most downloaded first.
    let res = app.call(Method::GET, &search, Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body[0]["fileId"], 8);
    assert_eq!(res.body[1]["release"], "Heat.1995.BluRay");
    assert!(
        seen.lock().unwrap().iter().any(|q| q == "languages=sv&tmdb_id=949"),
        "{:?}",
        seen.lock().unwrap()
    );
    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/items/{}/subtitles/search?language=svenska", seeded.movie_id),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Download: saved next to the video, signed in, and a new track.
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/subtitles/download", seeded.movie_id),
            Some(&admin),
            Some(json!({ "fileId": 7, "language": "sv" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    assert_eq!(res.body["fileName"], "Heat (1995).sv.srt");
    assert_eq!(res.body["remaining"], 4);
    let index = res.body["streamIndex"].as_u64().unwrap();
    let saved = std::path::Path::new(&app.media_dir("Movies")).join("Heat (1995).sv.srt");
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), SRT);
    assert!(seen.lock().unwrap().iter().any(|q| q == "download 7 Some(\"Bearer jwt\")"));

    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/stream/subtitles/{}/{index}.vtt", seeded.movie_part_id),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert!(res.body.as_str().unwrap().contains("Hej där"), "{}", res.body);
    let detail = app
        .call(Method::GET, &format!("/api/v1/items/{}", seeded.movie_id), Some(&admin), None)
        .await;
    let streams = detail.body["versions"][0]["parts"][0]["streams"].as_array().unwrap();
    assert!(streams.iter().any(|s| s["idx"] == index && s["language"] == "sv"), "{streams:?}");

    // A second one in the same language gets its own name.
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/subtitles/download", seeded.movie_id),
            Some(&admin),
            Some(json!({ "fileId": 8, "language": "sv" })),
        )
        .await;
    assert_eq!(res.body["fileName"], "Heat (1995).sv.1.srt");
}
