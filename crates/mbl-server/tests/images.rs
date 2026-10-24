#![allow(clippy::unwrap_used)]

mod common;

use std::io::Cursor;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use serde_json::json;

fn write_png(path: &std::path::Path) {
    let img =
        image::RgbaImage::from_fn(400, 600, |x, _| image::Rgba([(x % 255) as u8, 40, 90, 255]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(path, out).unwrap();
}

/// Library + movie item + one local poster artwork; returns (admin token, artwork id, library id).
async fn seed(app: &TestApp) -> (String, i64, i64) {
    let admin = app.setup_admin().await;
    let root = app.media_dir("Movies");
    let lib = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(json!({ "name": "M", "kind": "movie", "paths": [root] })),
        )
        .await;
    let lib_id = lib.body["id"].as_i64().unwrap();
    let poster = app.dir.path().join("media/Movies/poster.png");
    write_png(&poster);

    let db_path = app.dir.path().join("data/mbl.db");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path.display())).await.unwrap();
    let item: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'movie', 'Heat', 'heat', 0, 0) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let art: i64 = sqlx::query_scalar(
        "INSERT INTO artwork (item_id, kind, source_url, selected) VALUES (?, 'poster', ?, 1) RETURNING id",
    )
    .bind(item)
    .bind(format!("file:{}", poster.display()))
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    (admin, art, lib_id)
}

#[tokio::test]
async fn serves_resized_artwork_with_caching_headers() {
    let app = TestApp::new().await;
    let (admin, art, _) = seed(&app).await;

    let res = app
        .send(
            Request::get(format!("/api/v1/images/{art}?w=300"))
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.headers[header::CONTENT_TYPE], "image/jpeg");
    let etag = res.headers[header::ETAG].to_str().unwrap().to_owned();
    assert!(etag.contains("-320-80"), "width snapped to 320: {etag}");

    let again = app
        .send(
            Request::get(format!("/api/v1/images/{art}?w=300"))
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);

    let hash = etag.trim_matches('"').split('-').next().unwrap().to_owned();
    let immutable = app
        .send(
            Request::get(format!("/api/v1/images/{art}?w=300&v={hash}"))
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(immutable.headers[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));
}

#[tokio::test]
async fn requires_auth_and_hides_unknown_ids() {
    let app = TestApp::new().await;
    let (admin, art, _) = seed(&app).await;
    let anon = app.call(Method::GET, &format!("/api/v1/images/{art}"), None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    let missing = app.call(Method::GET, "/api/v1/images/9999", Some(&admin), None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
