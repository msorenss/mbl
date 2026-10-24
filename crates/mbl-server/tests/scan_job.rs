#![allow(clippy::unwrap_used)]

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use mbl_server::Config;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

async fn call(
    router: &Router,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn scan_endpoint_runs_a_scan_job() {
    let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/out/Movies/Big Test (2020)/Big Test (2020).mp4");
    if !sample.exists() || mbl_probe::FfTools::locate("", "").await.is_err() {
        eprintln!("skipping: needs ffprobe and `cargo xtask gen-media --quick`");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let movie_dir = dir.path().join("media/Big Test (2020)");
    std::fs::create_dir_all(&movie_dir).unwrap();
    std::fs::copy(&sample, movie_dir.join("Big Test (2020).mp4")).unwrap();

    let mut config = Config::default();
    config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
    let stop = CancellationToken::new();
    let (router, db) =
        mbl_server::build(config).await.unwrap().start_jobs(stop.clone()).await.unwrap();

    let setup = Request::post("/api/v1/system/setup")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "username": "admin", "password": "admin-password" }).to_string()))
        .unwrap();
    let res = router.clone().oneshot(setup).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let token =
        serde_json::from_slice::<Value>(&bytes).unwrap()["token"].as_str().unwrap().to_owned();

    let (status, lib) = call(
        &router,
        Method::POST,
        "/api/v1/libraries",
        &token,
        Some(json!({ "name": "Movies", "kind": "movie", "paths": [dir.path().join("media")] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{lib}");
    let lib_id = lib["id"].as_i64().unwrap();

    let (status, body) =
        call(&router, Method::POST, &format!("/api/v1/libraries/{lib_id}/scan"), &token, None)
            .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job_id = body["jobId"].as_i64().unwrap();

    let mut status = String::new();
    for _ in 0..300 {
        status = sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(&db.read)
            .await
            .unwrap();
        if status == "done" || status == "failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(status, "done");
    let movies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE kind = 'movie'")
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(movies, 1);
    let queued_match: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'metadata.match'")
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert_eq!(queued_match, 1, "new movie is queued for metadata matching");
    stop.cancel();
}

#[tokio::test]
async fn scan_requires_admin_and_is_deduplicated() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("media")).unwrap();
    let mut config = Config::default();
    config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
    let router = mbl_server::build(config).await.unwrap().router;

    let setup = Request::post("/api/v1/system/setup")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "username": "admin", "password": "admin-password" }).to_string()))
        .unwrap();
    let bytes = router
        .clone()
        .oneshot(setup)
        .await
        .unwrap()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let admin =
        serde_json::from_slice::<Value>(&bytes).unwrap()["token"].as_str().unwrap().to_owned();

    let (_, lib) = call(
        &router,
        Method::POST,
        "/api/v1/libraries",
        &admin,
        Some(json!({ "name": "Movies", "kind": "movie", "paths": [dir.path().join("media")] })),
    )
    .await;
    let uri = format!("/api/v1/libraries/{}/scan", lib["id"]);
    let (s1, a) = call(&router, Method::POST, &uri, &admin, None).await;
    let (s2, b) = call(&router, Method::POST, &uri, &admin, None).await;
    assert_eq!((s1, s2), (StatusCode::ACCEPTED, StatusCode::ACCEPTED));
    assert_eq!(a["jobId"], b["jobId"], "pending scan is reused");

    let (_, user) = call(
        &router,
        Method::POST,
        "/api/v1/users",
        &admin,
        Some(json!({ "username": "anna", "password": "anna-password" })),
    )
    .await;
    assert!(user["id"].is_number());
    let login = Request::post("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "username": "anna", "password": "anna-password" }).to_string()))
        .unwrap();
    let bytes = router
        .clone()
        .oneshot(login)
        .await
        .unwrap()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let anna =
        serde_json::from_slice::<Value>(&bytes).unwrap()["token"].as_str().unwrap().to_owned();
    let (s, _) = call(&router, Method::POST, &uri, &anna, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&router, Method::POST, "/api/v1/libraries/999/scan", &admin, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
