//! A real `mbl-worker` encodes an HLS session over HTTP; when it disappears the server
//! finishes the session locally.
#![allow(clippy::unwrap_used)]
mod common;

use std::net::SocketAddr;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use mbl_core::{WorkerEncoder, WorkerId};
use serde_json::json;
use tokio_util::sync::CancellationToken;

async fn twenty_second_source(app: &TestApp, db: &mbl_db::Db, admin: &str) -> i64 {
    let seeded = seed_playback_fixtures(app, db, admin).await;
    let path: String = sqlx::query_scalar("SELECT path FROM media_parts WHERE id=?")
        .bind(seeded.movie_part_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=s=160x90:r=24:d=20"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=20"])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "24", "-c:a", "aac"])
        .arg(&path)
        .status()
        .await
        .unwrap();
    assert!(status.success());
    seeded.movie_id
}

#[tokio::test]
async fn worker_encodes_session_and_server_takes_over_when_it_leaves() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let movie = twenty_second_source(&app, &db, &admin).await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let service = app.router.clone().into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, service).await });

    let code = app.call(Method::POST, "/api/v1/workers/pairing-code", Some(&admin), None).await;
    let paired = app
        .call(
            Method::POST,
            "/api/v1/workers/pair",
            None,
            Some(json!({
                "pairingCode": code.body["code"],
                "name": "test worker",
                "capabilities": {
                    "gpuVendor": "Cpu", "gpuDeviceName": "CPU",
                    "supportedVideoDecoders": [], "supportedVideoEncoders": ["software/h264"],
                    "supportsHardwareTonemap": false, "maxConcurrentSessions": 1
                }
            })),
        )
        .await;
    assert_eq!(paired.status, StatusCode::OK, "{}", paired.body);
    let worker_id = WorkerId(paired.body["workerId"].as_i64().unwrap());
    let config = mbl_worker::WorkerConfigFile {
        server_url: format!("http://{addr}"),
        worker_id,
        auth_token: paired.body["authToken"].as_str().unwrap().to_owned(),
        name: "test worker".into(),
    };
    let work = tempfile::tempdir().unwrap();
    let opts = mbl_worker::DaemonOptions {
        ffmpeg: "ffmpeg".into(),
        encoders: vec![WorkerEncoder { backend: "software".into(), codec: "h264".into() }],
        max_jobs: 1,
        pause_while_gaming: false,
        work_dir: work.path().to_path_buf(),
    };
    let stop = CancellationToken::new();
    let daemon = tokio::spawn(mbl_worker::run_daemon(config, opts, stop.clone()));

    let dispatcher = services.workers.dispatcher().unwrap().clone();
    for _ in 0..100 {
        if dispatcher.workers().iter().any(|w| w.worker_id == worker_id && w.online) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // A low bitrate cap forces a video encode, which goes to the worker.
    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId": movie, "waitForReady": true, "maxBitrate": 300000})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    assert_eq!(start.body["method"], "transcode", "{}", start.body);
    let id = start.body["sessionId"].as_str().unwrap().to_owned();
    let session = services.stream.hls_session(&id).unwrap();
    let runtime = session.runtime_status().await;
    assert_eq!(runtime.remote_worker.map(|(w, _)| w), Some(worker_id), "encoded remotely");
    assert!(session.ffmpeg_pid().await.is_none(), "no local FFmpeg while the worker encodes");
    assert!(session.is_remote());
    let first = session.request_segment("0", 0).await.unwrap();
    assert!(std::fs::metadata(&first).unwrap().len() > 0);
    assert!(session.dir.join("0/init.mp4").exists());
    // The uploaded output is real, decodable media.
    let init = std::fs::read(session.dir.join("0/init.mp4")).unwrap();
    let mut joined = init.clone();
    joined.extend(std::fs::read(&first).unwrap());
    let probe = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut probe = probe;
    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = probe.stdin.take().unwrap();
        stdin.write_all(&joined).await.unwrap();
    }
    let out = probe.wait_with_output().await.unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "h264");

    // The worker goes away and is revoked: the next segment is encoded locally.
    stop.cancel();
    daemon.await.unwrap().unwrap();
    let revoke = app
        .call(Method::DELETE, &format!("/api/v1/workers/{}", worker_id.0), Some(&admin), None)
        .await;
    assert_eq!(revoke.status, StatusCode::NO_CONTENT);
    let later = session.request_segment("0", 2).await.unwrap();
    assert!(std::fs::metadata(&later).unwrap().len() > 0);
    let runtime = session.runtime_status().await;
    assert!(runtime.remote_worker.is_none() && !session.is_remote());

    // A revoked worker can no longer push files into sessions.
    let rejected = app
        .call(
            Method::POST,
            "/api/v1/workers/poll",
            None,
            Some(json!({"encoders": [], "maxJobs": 1, "paused": false, "activeJobIds": []})),
        )
        .await;
    assert_eq!(rejected.status, StatusCode::UNAUTHORIZED);
    app.call(Method::POST, &format!("/api/v1/playback/{id}/stop"), Some(&admin), None).await;
}
