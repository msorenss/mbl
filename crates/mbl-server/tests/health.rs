#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mbl_server::Config;
use tower::ServiceExt;

async fn test_server() -> (mbl_server::Server, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.paths.data_dir = dir.path().to_str().unwrap().to_owned();
    (mbl_server::build(config).await.unwrap(), dir)
}

async fn get_json(router: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let res = router.oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn health_is_ok() {
    let (server, _dir) = test_server().await;
    let (status, body) = get_json(server.router, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!({ "status": "ok" }));
}

#[tokio::test]
async fn system_info_reports_version_and_platform() {
    let (server, _dir) = test_server().await;
    let (status, body) = get_json(server.router, "/api/v1/system/info").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "MBL");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["os"], std::env::consts::OS);
    assert!(body["platformProfile"].is_string());
}

#[tokio::test]
async fn health_fails_when_database_is_closed() {
    let (server, _dir) = test_server().await;
    server.db.close().await;
    let (status, body) = get_json(server.router, "/health").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "unavailable");
}

#[tokio::test]
async fn build_creates_data_layout() {
    let (server, dir) = test_server().await;
    assert!(dir.path().join("mbl.db").exists());
    assert!(dir.path().join("cache").is_dir());
    assert!(dir.path().join("transcode").is_dir());
    assert_eq!(server.paths.db_file.as_std_path(), dir.path().join("mbl.db"));
}

#[tokio::test]
async fn unknown_route_is_404() {
    let (server, _dir) = test_server().await;
    let res =
        server.router.oneshot(Request::get("/nope").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
