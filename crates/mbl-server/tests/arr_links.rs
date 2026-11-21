//! "Open in Radarr/Sonarr" links: only configured services, straight to the title when the
//! service has it, admin only.
#![allow(clippy::unwrap_used)]

mod common;

use axum::Router;
use axum::extract::{Path, Query};
use axum::http::{Method, StatusCode};
use axum::routing::get;
use common::{TestApp, seed_playback_fixtures};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Radarr knows movie 9 (TMDB 949, "heat-949"); Sonarr knows one show (TMDB 61125).
async fn fake_arr() -> String {
    let router = Router::new()
        .route(
            "/api/v3/movie/{id}",
            get(|Path(id): Path<i64>| async move {
                if id == 9 {
                    (StatusCode::OK, axum::Json(json!({ "id": 9, "titleSlug": "heat-949" })))
                } else {
                    (StatusCode::NOT_FOUND, axum::Json(Value::Null))
                }
            }),
        )
        .route(
            "/api/v3/movie",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                let found = q.get("tmdbId").map(String::as_str) == Some("949");
                axum::Json(if found {
                    json!([{ "id": 9, "titleSlug": "heat-949" }])
                } else {
                    json!([])
                })
            }),
        )
        .route(
            "/api/v3/series",
            get(|| async {
                axum::Json(json!([{ "id": 3, "tmdbId": 61125, "titleSlug": "dark-matter" }]))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

async fn configure(app: &TestApp, admin: &str, service: &str, url: &str) {
    let res = app
        .call(
            Method::PUT,
            &format!("/api/v1/requests/arr/{service}"),
            Some(admin),
            Some(json!({ "enabled": false, "url": url, "apiKey": "secret", "rootFolder": "" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
}

async fn request(app: &TestApp, token: &str, kind: &str, tmdb: i64) -> i64 {
    let res = app
        .call(
            Method::POST,
            "/api/v1/requests",
            Some(token),
            Some(json!({ "kind": kind, "tmdbId": tmdb, "title": "Title" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    res.body["id"].as_i64().unwrap()
}

#[tokio::test]
async fn links_open_the_title_in_the_download_service() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;

    // Nothing configured: no links.
    let res = app.call(Method::GET, "/api/v1/requests/arr-links", Some(&admin), None).await;
    assert_eq!(res.body, json!([]));
    let movie = request(&app, &admin, "movie", 949).await;
    let res = app
        .call(Method::GET, &format!("/api/v1/requests/{movie}/arr-link"), Some(&admin), None)
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    let url = fake_arr().await;
    configure(&app, &admin, "radarr", &url).await;
    configure(&app, &admin, "sonarr", &format!("{url}/")).await;
    let res = app.call(Method::GET, "/api/v1/requests/arr-links", Some(&admin), None).await;
    assert_eq!(
        res.body,
        json!([
            { "service": "radarr", "url": url, "direct": false },
            { "service": "sonarr", "url": url, "direct": false },
        ])
    );

    // A request Radarr has (by its own ID) opens the movie.
    let res = app
        .call(
            Method::PATCH,
            &format!("/api/v1/requests/{movie}/status"),
            Some(&admin),
            Some(json!({ "status": "approved", "arrId": 9 })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
    let res = app
        .call(Method::GET, &format!("/api/v1/requests/{movie}/arr-link"), Some(&admin), None)
        .await;
    assert_eq!(
        res.body,
        json!({ "service": "radarr", "url": format!("{url}/movie/heat-949"), "direct": true })
    );

    // A show Sonarr has, found by TMDB ID; one it does not have opens the start page.
    let show = request(&app, &admin, "show", 61125).await;
    let res = app
        .call(Method::GET, &format!("/api/v1/requests/{show}/arr-link"), Some(&admin), None)
        .await;
    assert_eq!(res.body["url"], format!("{url}/series/dark-matter"));
    let other = request(&app, &admin, "show", 1).await;
    let res = app
        .call(Method::GET, &format!("/api/v1/requests/{other}/arr-link"), Some(&admin), None)
        .await;
    assert_eq!(res.body, json!({ "service": "sonarr", "url": url, "direct": false }));

    // A library movie with a TMDB ID; an episode has no link.
    sqlx::query("UPDATE media_items SET tmdb_id = 949 WHERE id = ?")
        .bind(seeded.movie_id)
        .execute(&db.write)
        .await
        .unwrap();
    let path = format!("/api/v1/items/{}/arr-link", seeded.movie_id);
    let res = app.call(Method::GET, &path, Some(&admin), None).await;
    assert_eq!(res.body["url"], format!("{url}/movie/heat-949"));
    let res = app
        .call(Method::GET, &format!("/api/v1/items/{}/arr-link", seeded.ep1_id), Some(&admin), None)
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    // Other users never see the internal addresses.
    for path in [
        "/api/v1/requests/arr-links".to_owned(),
        format!("/api/v1/requests/{movie}/arr-link"),
        format!("/api/v1/items/{}/arr-link", seeded.movie_id),
    ] {
        let res = app.call(Method::GET, &path, Some(&viewer), None).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}");
    }
}
