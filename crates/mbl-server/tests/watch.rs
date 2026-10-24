#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use mbl_server::Config;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

async fn call(router: &Router, method: &str, uri: &str, token: Option<&str>, body: Value) -> Value {
    let mut req =
        Request::builder().method(method).uri(uri).header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let body = if body.is_null() { Body::empty() } else { Body::from(body.to_string()) };
    let res = router.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn created_file_is_scanned_within_15_seconds() {
    let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-media/out/Movies/Big Test (2020)/Big Test (2020).mp4");
    if !sample.exists() || mbl_probe::FfTools::locate("", "").await.is_err() {
        eprintln!("skipping: needs ffprobe and `cargo xtask gen-media --quick`");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media");
    std::fs::create_dir_all(&media).unwrap();
    let mut config = Config::default();
    config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
    let server = mbl_server::build(config).await.unwrap();

    // Libraries must exist before the watcher's first load.
    let router = server.router.clone();
    let setup = json!({ "username": "admin", "password": "admin-password" });
    let token = call(&router, "POST", "/api/v1/system/setup", None, setup).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let lib = json!({ "name": "Movies", "kind": "movie", "paths": [media.to_str().unwrap()] });
    let lib_id =
        call(&router, "POST", "/api/v1/libraries", Some(&token), lib).await["id"].as_i64().unwrap();

    let stop = CancellationToken::new();
    let (router, _db) = server.start_jobs(stop.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let movie_dir = media.join("Big Test (2020)");
    std::fs::create_dir_all(&movie_dir).unwrap();
    std::fs::copy(&sample, movie_dir.join("Big Test (2020).mp4")).unwrap();
    let copied = Instant::now();

    let uri = format!("/api/v1/libraries/{lib_id}/items");
    loop {
        let items = call(&router, "GET", &uri, Some(&token), Value::Null).await;
        if items["items"].as_array().is_some_and(|a| !a.is_empty()) {
            assert_eq!(items["items"][0]["title"], "Big Test");
            break;
        }
        assert!(copied.elapsed() < Duration::from_secs(15), "file not scanned within 15 s");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    stop.cancel();
}
