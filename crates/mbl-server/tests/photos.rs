#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;
#[tokio::test]
async fn photo_months_and_pages_apply_the_same_rating_and_library_policy() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let guest = app.create_user(&admin, "guest").await;
    let root = app.media_dir("Photos");
    let library = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(json!({"name":"Photos","kind":"photo","paths":[root]})),
        )
        .await;
    let id = library.body["id"].as_i64().unwrap();
    sqlx::query("UPDATE users SET content_rating_ceiling = 'PG' WHERE username = 'guest'")
        .execute(&db.write)
        .await
        .unwrap();
    let mut first = 0;
    for (title, rating) in [("A", "PG"), ("B", "PG"), ("Hidden", "R")] {
        let item:i64=sqlx::query_scalar("INSERT INTO media_items (library_id,kind,title,sort_title,content_rating,exif_date,added_at,updated_at) VALUES (?,'photo',?,?,?,?,?,?) RETURNING id").bind(id).bind(title).bind(title).bind(rating).bind(1721044800000_i64).bind(100).bind(100).fetch_one(&db.write).await.unwrap();
        if first == 0 {
            first = item;
        }
    }
    let timeline = app
        .call(Method::GET, &format!("/api/v1/libraries/{id}/timeline"), Some(&guest), None)
        .await;
    assert_eq!(timeline.status, StatusCode::OK);
    assert_eq!(timeline.body[0]["count"], 2);
    let page = app
        .call(
            Method::GET,
            &format!("/api/v1/libraries/{id}/photos?year=2024&month=7&limit=1"),
            Some(&guest),
            None,
        )
        .await;
    assert_eq!(page.status, StatusCode::OK);
    let cursor = page.body["nextCursor"].as_str().unwrap();
    let second = app
        .call(
            Method::GET,
            &format!("/api/v1/libraries/{id}/photos?year=2024&month=7&limit=1&cursor={cursor}"),
            Some(&guest),
            None,
        )
        .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_ne!(page.body["items"][0]["id"], second.body["items"][0]["id"]);
    assert_eq!(second.body["items"][0]["title"], "A");
    assert!(second.body["nextCursor"].is_null());
    assert_eq!(
        app.call(
            Method::GET,
            &format!("/api/v1/libraries/{id}/photos?year=2024&month=13"),
            Some(&guest),
            None
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let path = std::path::Path::new(&root).join("photo.png");
    image::RgbImage::new(8, 8).save(&path).unwrap();
    let version: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id,container) VALUES (?,'png') RETURNING id",
    )
    .bind(first)
    .fetch_one(&db.write)
    .await
    .unwrap();
    let part:i64=sqlx::query_scalar("INSERT INTO media_parts (version_id,path,size,mtime,probe) VALUES (?,?,1,0,'{}') RETURNING id").bind(version).bind(path.to_str().unwrap()).fetch_one(&db.write).await.unwrap();
    let picture =
        app.call(Method::GET, &format!("/api/v1/stream/direct/{part}"), Some(&guest), None).await;
    assert_eq!(picture.status, StatusCode::OK);
    assert_eq!(picture.headers["content-type"], "image/png");
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/stream/direct/{part}"), None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE users SET is_managed = 1 WHERE username = 'guest'")
        .execute(&db.write)
        .await
        .unwrap();
    for uri in [
        format!("/api/v1/libraries/{id}/timeline"),
        format!("/api/v1/libraries/{id}/photos"),
        format!("/api/v1/stream/direct/{part}"),
    ] {
        assert_eq!(
            app.call(Method::GET, &uri, Some(&guest), None).await.status,
            StatusCode::NOT_FOUND
        );
    }
}
