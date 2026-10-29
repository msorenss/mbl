#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use mbl_core::{Actor, UserId};
use mbl_jobs::ActivityProbe;
use serde_json::json;
use std::time::Duration;

async fn real_source(app: &TestApp, db: &mbl_db::Db, admin: &str) -> common::SeededShow {
    let seeded = seed_playback_fixtures(app, db, admin).await;
    let path: String = sqlx::query_scalar("SELECT path FROM media_parts WHERE id=?")
        .bind(seeded.movie_part_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert!(
        tokio::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=black:s=64x36:r=24:d=3",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=3",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-g",
                "24",
                "-c:a",
                "aac"
            ])
            .arg(&path)
            .status()
            .await
            .unwrap()
            .success()
    );
    // The cached caption deliberately no longer exists in the physical file.
    seeded
}
async fn history(db: &mbl_db::Db) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM watch_history").fetch_one(&db.read).await.unwrap()
}
#[tokio::test]
async fn ffmpeg_failure_before_ready_keeps_old_session_and_discards_candidate() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = real_source(&app, &db, &admin).await;
    let old = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id,"waitForReady":true})),
        )
        .await;
    assert_eq!(old.status, StatusCode::OK, "{}", old.body);
    let failed = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id,"waitForReady":true,"subtitleStreamIndex":2,
        "profile":{"name":"chrome","subtitles":[{"format":"srt","method":"burn"}]}})),
        )
        .await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE, "{}", failed.body);
    let active = services.playback.list_active(&Actor::admin(UserId(1)));
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].session_id, old.body["sessionId"].as_str().unwrap());
    assert_eq!(services.stream.hls_manager().unwrap().active_count(), 0);
    assert_eq!(history(&db).await, 0);
    assert_eq!(
        app.call(Method::GET, old.body["url"].as_str().unwrap(), None, None).await.status,
        StatusCode::OK
    );
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", active[0].session_id),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        history(&db).await,
        0,
        "discarded/unplayed candidates must not create watch history"
    );
}
#[tokio::test]
async fn ready_ack_contains_media_and_only_confirmed_playback_writes_history() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = real_source(&app, &db, &admin).await;
    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id,"waitForReady":true,"audioOffsetMs":250})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    let id = start.body["sessionId"].as_str().unwrap();
    let session = services.stream.hls_session(id).unwrap();
    assert!(session.dir.join("0/init.mp4").exists());
    assert!(session.dir.join("0/0.m4s").exists());
    let fragment = session.request_segment("0", 0).await.unwrap();
    assert!(std::fs::metadata(fragment).unwrap().len() > 0);
    let watch_before =
        mbl_db::repo::watch::get(&db, UserId(1), mbl_core::ItemId(seeded.movie_id)).await.unwrap();
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{id}/progress"),
        Some(&admin),
        Some(json!({"positionMs":0,"state":"buffering"})),
    )
    .await;
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{id}/stop"),
        Some(&admin),
        Some(json!({"positionMs":123000})),
    )
    .await;
    assert_eq!(history(&db).await, 0);
    let watch_after =
        mbl_db::repo::watch::get(&db, UserId(1), mbl_core::ItemId(seeded.movie_id)).await.unwrap();
    assert_eq!(watch_before, watch_after);
    assert!(session.ffmpeg_pid().await.is_none());
    assert!(
        session.request_segment("0", 0).await.is_err(),
        "a retained Arc cannot resurrect a stopped session"
    );
    assert!(!session.dir.exists());
    let started = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id,"waitForReady":true})),
        )
        .await;
    let id = started.body["sessionId"].as_str().unwrap();
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{id}/progress"),
        Some(&admin),
        Some(json!({"positionMs":1000,"state":"playing"})),
    )
    .await;
    app.call(Method::POST, &format!("/api/v1/playback/{id}/stop"), Some(&admin), None).await;
    assert_eq!(history(&db).await, 1);
    // A nonempty/readable direct source is required when startup validation is requested.
    let path: String = sqlx::query_scalar("SELECT path FROM media_parts WHERE id=?")
        .bind(seeded.movie_part_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    std::fs::write(path, []).unwrap();
    let empty = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id,"waitForReady":true})),
        )
        .await;
    assert_eq!(empty.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!services.playback.playback_active());
}

#[cfg(unix)]
async fn stalled_service(
    timeout: Duration,
) -> (
    TestApp,
    mbl_db::Db,
    mbl_service::PlaybackService,
    mbl_transcode::SessionManager,
    common::SeededShow,
) {
    use std::os::unix::fs::PermissionsExt;
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let binary = app.dir.path().join("stall.sh");
    std::fs::write(&binary, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = mbl_transcode::SessionManagerConfig::new(
        binary,
        camino::Utf8PathBuf::from_path_buf(app.dir.path().join("startup-hls")).unwrap(),
        mbl_platform::PlatformProfile::Generic,
    );
    config.segment_wait_timeout = timeout;
    let manager = mbl_transcode::SessionManager::new(config).unwrap();
    let playback = services.playback.with_stream(services.stream.with_hls(manager.clone()));
    (app, db, playback, manager, seeded)
}
#[cfg(unix)]
#[tokio::test]
async fn timed_out_start_releases_capacity_process_and_pending_activity() {
    let (_app, db, playback, manager, seeded) = stalled_service(Duration::from_millis(150)).await;
    let req = serde_json::from_value(
        json!({"itemId":seeded.movie_id,"waitForReady":true,"audioOffsetMs":250}),
    )
    .unwrap();
    assert!(playback.start(&Actor::admin(UserId(1)), req).await.is_err());
    assert_eq!(manager.active_count(), 0);
    assert_eq!(std::fs::read_dir(manager.config().transcode_dir.as_std_path()).unwrap().count(), 0);
    assert!(!playback.playback_active());
    assert_eq!(history(&db).await, 0);
}
#[cfg(unix)]
#[tokio::test]
async fn cancelling_start_cleans_up_even_if_another_task_keeps_the_session_arc() {
    let (_app, db, playback, manager, seeded) = stalled_service(Duration::from_secs(5)).await;
    let request = serde_json::from_value(
        json!({"itemId":seeded.movie_id,"waitForReady":true,"audioOffsetMs":250}),
    )
    .unwrap();
    let service = playback.clone();
    let start = tokio::spawn(async move { service.start(&Actor::admin(UserId(1)), request).await });
    let candidate = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(entry) = std::fs::read_dir(manager.config().transcode_dir.as_std_path())
                .unwrap()
                .flatten()
                .next()
                && let Some(session) = manager.get_session(&entry.file_name().to_string_lossy())
            {
                break session;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(playback.playback_active(), "pending startup must pause idle work");
    assert!(candidate.ffmpeg_pid().await.is_some());
    start.abort();
    assert!(start.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(3), async {
        while manager.active_count() > 0 || candidate.dir.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!playback.playback_active());
    assert!(candidate.ffmpeg_pid().await.is_none());
    assert!(candidate.request_segment("0", 0).await.is_err());
    assert_eq!(history(&db).await, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn shutdown_rejects_pending_start_and_releases_its_capacity() {
    let (_app, db, playback, manager, seeded) = stalled_service(Duration::from_secs(5)).await;
    let req = serde_json::from_value(
        json!({"itemId":seeded.movie_id,"waitForReady":true,"audioOffsetMs":250}),
    )
    .unwrap();
    let service = playback.clone();
    let start = tokio::spawn(async move { service.start(&Actor::admin(UserId(1)), req).await });
    let candidate = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(entry) = std::fs::read_dir(manager.config().transcode_dir.as_std_path())
                .unwrap()
                .flatten()
                .next()
                && let Some(session) = manager.get_session(&entry.file_name().to_string_lossy())
            {
                break session;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    playback.shutdown().await;
    assert!(tokio::time::timeout(Duration::from_secs(1), start).await.unwrap().unwrap().is_err());
    assert_eq!(manager.active_count(), 0);
    assert!(candidate.ffmpeg_pid().await.is_none());
    assert!(!candidate.dir.exists());
    assert!(!playback.playback_active());
    assert!(playback.list_active(&Actor::admin(UserId(1))).is_empty());
    assert_eq!(history(&db).await, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn queued_retention_does_not_block_shutdown_of_a_stalled_start() {
    let (_app, db, playback, manager, seeded) = stalled_service(Duration::from_secs(60)).await;
    let req = serde_json::from_value(
        json!({"itemId":seeded.movie_id,"waitForReady":true,"audioOffsetMs":250}),
    )
    .unwrap();
    let start_service = playback.clone();
    let start =
        tokio::spawn(async move { start_service.start(&Actor::admin(UserId(1)), req).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while manager.active_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let maintenance_service = playback.clone();
    let livetv = mbl_service::LiveTvService::new(std::sync::Arc::new(db.clone()));
    let maintenance = tokio::spawn(async move { maintenance_service.maintain_dvr(&livetv).await });
    let actor = Actor::admin(UserId(1));
    // A public heartbeat normally returns 404 immediately for this id. Blocking
    // proves retention has queued its writer behind the still-running start.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let progress =
                serde_json::from_value(json!({"positionMs":0,"state":"buffering"})).unwrap();
            if tokio::time::timeout(
                Duration::from_millis(25),
                playback.progress(&actor, "absent-session", progress),
            )
            .await
            .is_err()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), playback.shutdown()).await.unwrap();
    assert!(start.await.unwrap().is_err());
    maintenance.await.unwrap().unwrap();
    assert_eq!(manager.active_count(), 0);
    assert_eq!(history(&db).await, 0);
}
