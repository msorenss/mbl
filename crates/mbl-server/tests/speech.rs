//! Speech to text on the server: without a Whisper model the server says so, and recordings are
//! limited in size.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;

#[tokio::test]
async fn speech_recognition_reports_what_the_server_can_do() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;

    let res = app.call(Method::GET, "/api/v1/speech/status", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK);
    // Test machines have no Whisper model on an NPU.
    if res.body["serverRecognition"] == false {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/speech/transcribe?language=sv")
            .header(header::AUTHORIZATION, format!("Bearer {admin}"))
            .header(header::CONTENT_TYPE, "audio/webm")
            .body(Body::from(vec![0u8; 1024]))
            .unwrap();
        assert_eq!(app.send(req).await.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    // Too large a recording is refused before anything is decoded.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/speech/transcribe")
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .header(header::CONTENT_TYPE, "audio/webm")
        .body(Body::from(vec![0u8; 5 * 1024 * 1024]))
        .unwrap();
    assert_eq!(app.send(req).await.status, StatusCode::PAYLOAD_TOO_LARGE);

    let res = app.call(Method::GET, "/api/v1/speech/status", None, None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}
