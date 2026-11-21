//! Screenshots for the AI: off by default, then a real JPEG at the viewer's position for REST
//! and MCP (`get_companion_frame`, also without a session ID).

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn screenshots_are_off_until_the_viewer_turns_them_on() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;

    // A real minute of video in place of the fixture's placeholder file.
    let movie = std::path::Path::new(&app.media_dir("Movies")).join("Heat (1995).mp4");
    let made = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg("testsrc=duration=60:size=640x360:rate=10")
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "ultrafast"])
        .arg(&movie)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());

    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId": fixture.movie_id, "profile": "chrome", "startMs": 42000})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    let session = start.body["sessionId"].as_str().unwrap().to_owned();
    let base = format!("/api/v1/companion/sessions/{session}");

    let res = app.call(Method::GET, &format!("{base}/frame"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::CONFLICT, "off by default: {}", res.body);
    let context = app.call(Method::GET, &format!("{base}/context"), Some(&admin), None).await;
    assert_eq!(context.body["frames"], "off");
    // The scene is analysed on the server either way (no picture leaves it): by the NPU when
    // there is one, otherwise only as a colour guess, and the two are told apart.
    let scene = &context.body["scene"];
    assert_eq!(scene["positionMs"], 42000, "{}", context.body);
    let backend = scene["backend"].as_str().unwrap();
    assert_eq!(scene["npu"], backend != "cpu-heuristic");
    if backend == "cpu-heuristic" {
        assert_eq!(scene["objects"], serde_json::json!([]));
    }

    let res = app
        .call(
            Method::PUT,
            &format!("{base}/mode"),
            Some(&admin),
            Some(json!({"mode": "on", "frames": "30s"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("{base}/frame"))
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .body(Body::empty())
        .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
    assert_eq!(res.headers().get("x-mbl-frame-position").unwrap(), "42000");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "a JPEG");

    // MCP: without a session ID the caller's own playback is used; the picture is an image block.
    let mut client = mbl_mcp::McpClient::connect_router(app.router.clone(), Some(admin));
    client.initialize().await.unwrap();
    let result = client.call_tool("get_companion_frame", json!({})).await.unwrap();
    let data: Value = result.structured_content.clone().unwrap();
    assert_eq!(data["positionMs"], 42000);
    let image = result.content.iter().find(|c| c.kind == "image").expect("an image block");
    assert_eq!(image.mime_type.as_deref(), Some("image/jpeg"));
    assert!(image.data.as_deref().unwrap().starts_with("/9j/"), "base64 JPEG");
}
