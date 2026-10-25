#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
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

#[tokio::test]
async fn root_serves_embedded_index_html() {
    let (server, _dir) = test_server().await;
    let req = Request::get("/").body(Body::empty()).unwrap();
    let res = server.router.oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(content_type.contains("text/html"));

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes);
    assert!(html.to_lowercase().contains("<!doctype html>"));
    assert!(html.contains("id=\"root\""));
}

#[tokio::test]
async fn spa_navigation_falls_back_to_index_html() {
    let (server, _dir) = test_server().await;
    let req = Request::get("/libraries")
        .header(ACCEPT, "text/html,application/xhtml+xml")
        .body(Body::empty())
        .unwrap();
    let res = server.router.oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(content_type.contains("text/html"));

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes);
    assert!(html.to_lowercase().contains("<!doctype html>"));
}

#[tokio::test]
async fn non_html_missing_route_returns_404() {
    let (server, _dir) = test_server().await;
    let req = Request::get("/nonexistent/file.bin").body(Body::empty()).unwrap();
    let res = server.router.oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_missing_route_never_falls_back_to_html() {
    let (server, _dir) = test_server().await;
    let req = Request::get("/api/v1/nonexistent")
        .header(ACCEPT, "text/html")
        .body(Body::empty())
        .unwrap();
    let res = server.router.oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
