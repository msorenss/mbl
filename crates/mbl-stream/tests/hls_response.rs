#![allow(clippy::unwrap_used)]

use axum::http::{StatusCode, header};
use http_body_util::BodyExt;
use mbl_stream::hls::{MIME_M4S, serve_segment_file};

#[tokio::test]
async fn segment_streams_complete_file_with_length_and_no_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("0.m4s");
    let bytes = vec![42; 300 * 1024];
    tokio::fs::write(&path, &bytes).await.unwrap();

    let response = serve_segment_file(&path, MIME_M4S).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], MIME_M4S);
    assert_eq!(response.headers()[header::CONTENT_LENGTH], bytes.len().to_string());
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.into_body().collect().await.unwrap().to_bytes(), bytes);
}
