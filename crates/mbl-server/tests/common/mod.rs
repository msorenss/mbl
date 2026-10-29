//! Shared helpers for HTTP-level integration tests.
#![allow(dead_code, clippy::unwrap_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use mbl_server::Config;
use serde_json::{Value, json};
use tower::ServiceExt;

pub struct TestApp {
    pub router: Router,
    pub dir: tempfile::TempDir,
}

pub struct Res {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::new_with_services().await.0
    }

    pub async fn new_with_services() -> (Self, mbl_db::Db, mbl_service::Services) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
        let server = mbl_server::build(config).await.unwrap();
        (Self { router: server.router, dir }, server.db, server.services)
    }

    pub async fn send(&self, req: Request<Body>) -> Res {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
        };
        Res { status, headers, body }
    }

    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Res {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        self.send(req).await
    }

    /// Run setup and return the admin token.
    pub async fn setup_admin(&self) -> String {
        let res = self
            .call(
                Method::POST,
                "/api/v1/system/setup",
                None,
                Some(json!({ "username": "admin", "password": "admin-password" })),
            )
            .await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        res.body["token"].as_str().unwrap().to_owned()
    }

    pub async fn login(&self, username: &str, password: &str) -> Res {
        self.call(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "username": username, "password": password, "deviceName": "test" })),
        )
        .await
    }

    /// Create a non-admin user and return their token.
    pub async fn create_user(&self, admin: &str, username: &str) -> String {
        let password = format!("{username}-password");
        let res = self
            .call(
                Method::POST,
                "/api/v1/users",
                Some(admin),
                Some(json!({ "username": username, "password": password })),
            )
            .await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        self.login(username, &password).await.body["token"].as_str().unwrap().to_owned()
    }

    /// Create a directory under the test's temp dir and return its path.
    pub fn media_dir(&self, rel: &str) -> String {
        let path = self.dir.path().join("media").join(rel);
        std::fs::create_dir_all(&path).unwrap();
        path.to_str().unwrap().to_owned()
    }

    /// Match the server's configured spelling, including Windows separators.
    /// DVR library roots are persisted and compared as strings in SQLite.
    pub fn recordings_dir(&self) -> std::path::PathBuf {
        self.dir.path().join("data").join("recordings")
    }
}

pub struct SeededShow {
    pub show_id: i64,
    pub season_id: i64,
    pub ep1_id: i64,
    pub ep1_part_id: i64,
    pub ep2_id: i64,
    pub movie_id: i64,
    pub movie_part_id: i64,
}

pub async fn seed_playback_fixtures(app: &TestApp, db: &mbl_db::Db, admin: &str) -> SeededShow {
    let movies_dir = app.media_dir("Movies");
    let movie_path = std::path::Path::new(&movies_dir).join("Heat (1995).mp4");
    std::fs::write(&movie_path, b"synthetic mp4 content").unwrap();

    let lib = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(admin),
            Some(json!({ "name": "Movies", "kind": "movie", "paths": [movies_dir] })),
        )
        .await;
    assert_eq!(lib.status, StatusCode::CREATED);
    let lib_id = lib.body["id"].as_i64().unwrap();

    // 100-minute movie (6_000_000 ms) with H.264 + AAC + external SRT + chapters
    let movie_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, duration_ms, added_at, updated_at)
         VALUES (?, 'movie', 'Heat', 'heat', 6000000, 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let movie_ver_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mp4', 'h264', 'aac', 1920, 1080) RETURNING id",
    )
    .bind(movie_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let chapters_json = serde_json::to_string(&vec![
        json!({ "startMs": 0, "endMs": 90000, "title": "Opening Credits" }),
        json!({ "startMs": 5700000, "endMs": 6000000, "title": "End Credits" }),
    ])
    .unwrap();

    let movie_probe = serde_json::to_string(&json!({
        "container": "mp4",
        "durationMs": 6_000_000,
        "bitrate": 5_000_000,
        "size": 1024,
        "streams": [
            {
                "index": 0,
                "kind": "video",
                "codec": "h264",
                "profile": "high",
                "level": 41,
                "default": true,
                "forced": false,
                "width": 1920,
                "height": 1080,
                "bitDepth": 8,
                "interlaced": false
            },
            {
                "index": 1,
                "kind": "audio",
                "codec": "aac",
                "language": "en",
                "default": true,
                "forced": false,
                "channels": 2,
                "interlaced": false
            },
            {
                "index": 2,
                "kind": "subtitle",
                "codec": "srt",
                "language": "en",
                "title": "English",
                "default": true,
                "forced": false,
                "interlaced": false
            }
        ],
        "chapters": [
            { "startMs": 0, "endMs": 90000, "title": "Opening" },
            { "startMs": 5700000, "endMs": 6000000, "title": "End Credits" }
        ]
    }))
    .unwrap();

    let movie_part_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, probe, chapters, part_index)
         VALUES (?, ?, 1024, 1, 6000000, ?, ?, 0) RETURNING id",
    )
    .bind(movie_ver_id)
    .bind(movie_path.to_str().unwrap())
    .bind(&movie_probe)
    .bind(&chapters_json)
    .fetch_one(&db.write)
    .await
    .unwrap();

    // Show -> Season 1 -> Episode 1 (25 min = 1_500_000 ms, MKV + HEVC) & Episode 2
    let show_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'show', 'Show', 'show', 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let season_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, parent_id, idx, title, sort_title, added_at, updated_at)
         VALUES (?, 'season', ?, 1, 'Season 1', 'season 1', 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .bind(show_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ep1_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, parent_id, idx, parent_idx, title, sort_title, duration_ms, added_at, updated_at)
         VALUES (?, 'episode', ?, 1, 1, 'Ep 1', 'ep 1', 1500000, 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .bind(season_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ep2_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, parent_id, idx, parent_idx, title, sort_title, duration_ms, added_at, updated_at)
         VALUES (?, 'episode', ?, 2, 1, 'Ep 2', 'ep 2', 1500000, 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .bind(season_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ep1_path = std::path::Path::new(&movies_dir).join("Show - S01E01.mkv");
    std::fs::write(&ep1_path, b"synthetic mkv content").unwrap();

    let ep1_probe = serde_json::to_string(&json!({
        "container": "mkv",
        "durationMs": 1_500_000,
        "bitrate": 8_000_000,
        "size": 2048,
        "streams": [
            {
                "index": 0,
                "kind": "video",
                "codec": "hevc",
                "profile": "main10",
                "level": 150,
                "default": true,
                "forced": false,
                "width": 3840,
                "height": 2160,
                "bitDepth": 10,
                "interlaced": false
            },
            {
                "index": 1,
                "kind": "audio",
                "codec": "ac3",
                "language": "en",
                "default": true,
                "forced": false,
                "channels": 6,
                "interlaced": false
            }
        ],
        "chapters": []
    }))
    .unwrap();

    let ep1_ver_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mkv', 'hevc', 'ac3', 3840, 2160) RETURNING id",
    )
    .bind(ep1_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ep1_part_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, probe, chapters, part_index)
         VALUES (?, ?, 2048, 1, 1500000, ?, '[]', 0) RETURNING id",
    )
    .bind(ep1_ver_id)
    .bind(ep1_path.to_str().unwrap())
    .bind(&ep1_probe)
    .fetch_one(&db.write)
    .await
    .unwrap();

    SeededShow { show_id, season_id, ep1_id, ep1_part_id, ep2_id, movie_id, movie_part_id }
}

/// An isolated HTTP tuner fixture: real MPEG-TS bytes, no external network or tuner hardware.
pub struct TestLiveSource {
    pub url: String,
    pub bytes: axum::body::Bytes,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for TestLiveSource {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub async fn live_source() -> TestLiveSource {
    live_source_with_duration(6).await
}
pub async fn live_source_with_duration(seconds: u32) -> TestLiveSource {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.ts");
    let video = format!("testsrc=size=128x72:rate=50:duration={seconds}");
    let audio = format!("sine=frequency=880:sample_rate=48000:duration={seconds}");
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &video,
            "-f",
            "lavfi",
            "-i",
            &audio,
            "-vf",
            "format=yuv420p,tinterlace=mode=interleave_top",
            "-c:v",
            "mpeg2video",
            "-flags",
            "+ildct+ilme",
            "-threads",
            "1",
            "-g",
            "25",
            "-c:a",
            "ac3",
            "-ac",
            "2",
            "-f",
            "mpegts",
        ])
        .arg(&path)
        .status()
        .await
        .unwrap();
    assert!(status.success());
    let probe = tokio::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_name,field_order", "-of", "json"])
        .arg(&path)
        .output()
        .await
        .unwrap();
    assert!(probe.status.success());
    let info: Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert!(info["streams"].as_array().unwrap().iter().any(|s| s["codec_name"] == "mpeg2video"
        && s["field_order"] != "progressive"
        && s["field_order"] != "unknown"));
    assert!(info["streams"].as_array().unwrap().iter().any(|s| s["codec_name"] == "ac3"));
    let bytes = axum::body::Bytes::from(tokio::fs::read(path).await.unwrap());
    let source_bytes = bytes.clone();
    let router = axum::Router::new().route(
        "/live.ts",
        axum::routing::get(move || {
            let bytes = bytes.clone();
            async move { ([("Content-Type", "video/mp2t")], bytes) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/live.ts", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    TestLiveSource { url, bytes: source_bytes, task }
}
