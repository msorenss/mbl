#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

/// A movie library with one movie that has a child item (enough to exercise `/children`).
async fn seed(app: &TestApp, admin: &str) -> (i64, i64, i64) {
    let movies = app.media_dir("Movies");
    let lib = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(admin),
            Some(json!({ "name": "Movies", "kind": "movie", "paths": [movies] })),
        )
        .await;
    let lib_id = lib.body["id"].as_i64().unwrap();
    let db_path = app.dir.path().join("data/mbl.db");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path.display())).await.unwrap();
    let movie: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, year, added_at, updated_at)
         VALUES (?, 'movie', 'Heat', 'heat', 1995, 1, 1) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let season: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, parent_id, title, sort_title, idx, added_at, updated_at)
         VALUES (?, 'season', ?, 'Specials', 'specials', 0, 1, 1) RETURNING id",
    )
    .bind(lib_id)
    .bind(movie)
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    (lib_id, movie, season)
}

#[tokio::test]
async fn browse_endpoints() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (lib, movie, season) = seed(&app, &admin).await;

    let list = app
        .call(
            Method::GET,
            &format!("/api/v1/libraries/{lib}/items?sort=year&limit=10"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(list.status, StatusCode::OK, "{:?}", list.body);
    assert_eq!(list.body["items"][0]["title"], "Heat");
    assert_eq!(list.body["nextCursor"], serde_json::Value::Null);

    let bad = app
        .call(Method::GET, &format!("/api/v1/libraries/{lib}/items?sort=bogus"), Some(&admin), None)
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    let detail = app.call(Method::GET, &format!("/api/v1/items/{movie}"), Some(&admin), None).await;
    assert_eq!(detail.status, StatusCode::OK);
    assert_eq!(detail.body["year"], 1995);
    assert_eq!(detail.body["watch"]["viewCount"], 0);

    let children =
        app.call(Method::GET, &format!("/api/v1/items/{movie}/children"), Some(&admin), None).await;
    assert_eq!(children.body[0]["id"], season);

    let search = app.call(Method::GET, "/api/v1/search?q=hea", Some(&admin), None).await;
    assert_eq!(search.body[0]["kind"], "movie");
    assert_eq!(search.body[0]["items"][0]["id"], movie);

    let hubs = app.call(Method::GET, "/api/v1/hubs/home", Some(&admin), None).await;
    assert_eq!(hubs.status, StatusCode::OK);
    assert_eq!(hubs.body[0]["key"], format!("recentlyAdded.{lib}"));

    for uri in
        ["/api/v1/items/999999", "/api/v1/items/999999/children", "/api/v1/libraries/999999/items"]
    {
        let res = app.call(Method::GET, uri, Some(&admin), None).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{uri}");
    }
    let anon = app.call(Method::GET, "/api/v1/hubs/home", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn openapi_and_docs_are_served() {
    let app = TestApp::new().await;
    let spec = app.call(Method::GET, "/api/openapi.json", None, None).await;
    assert_eq!(spec.status, StatusCode::OK);
    assert!(spec.body["paths"]["/api/v1/items/{id}"]["get"].is_object());
    assert!(spec.body["components"]["schemas"]["ItemDetail"].is_object());
    let docs = app.call(Method::GET, "/api/docs", None, None).await;
    assert_eq!(docs.status, StatusCode::OK);
    assert!(docs.body.as_str().unwrap().contains("api-reference"));
}
