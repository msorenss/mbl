//! "Continue watching" snapshots: a real JPEG of the video where the viewer stopped.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn get(
    app: &TestApp,
    uri: &str,
    token: Option<&str>,
) -> (StatusCode, Option<String>, Vec<u8>) {
    let mut req = Request::builder().method(Method::GET).uri(uri);
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let res = app.router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = res.status();
    let kind = res.headers().get(header::CONTENT_TYPE).map(|v| v.to_str().unwrap().to_owned());
    (status, kind, res.into_body().collect().await.unwrap().to_bytes().to_vec())
}

#[tokio::test]
async fn a_snapshot_shows_the_video_where_the_viewer_stopped() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;
    let movie = std::path::Path::new(&app.media_dir("Movies")).join("Heat (1995).mp4");
    let made = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg("testsrc=duration=60:size=640x360:rate=10")
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "ultrafast"])
        .arg(&movie)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());
    let uri = format!("/api/v1/items/{}/snapshot?at=42345", fixture.movie_id);

    let (status, kind, first) = get(&app, &uri, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&first));
    assert_eq!(kind.as_deref(), Some("image/jpeg"));
    assert_eq!(&first[..2], &[0xFF, 0xD8], "a JPEG");
    let picture = image::load_from_memory(&first).expect("decodes");
    assert!(picture.width() <= 480, "scaled for a card: {}", picture.width());

    // The same 10-second step is the same picture; another step is another one.
    let again = format!("/api/v1/items/{}/snapshot?at=49999", fixture.movie_id);
    let (status, _, second) = get(&app, &again, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first, second);
    let earlier = format!("/api/v1/items/{}/snapshot?at=10000", fixture.movie_id);
    let (status, _, third) = get(&app, &earlier, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(first, third);

    let (status, _, _) = get(&app, &uri, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = get(&app, "/api/v1/items/999999/snapshot?at=0", Some(&admin)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
