#![cfg(unix)]
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::Method;
use common::{TestApp, seed_playback_fixtures};
use mbl_core::{Actor, UserId};
use mbl_jobs::ActivityProbe;
use mbl_service::playback::{PlaybackProgressRequest, PlaybackRequest, PlaybackState};
use serde_json::json;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

struct Fixture {
    server: mbl_server::Server,
    app: TestApp,
    playback: mbl_service::PlaybackService,
    manager: mbl_transcode::SessionManager,
    item: i64,
}
async fn fixture() -> Fixture {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let mut config = mbl_server::Config::default();
    config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
    config.scanner.watch = false;
    config.server.port = 0;
    config.server.bind = "127.0.0.1".into();
    let mut server = mbl_server::build(config).await.unwrap();
    let app = TestApp { router: server.router.clone(), dir };
    let admin = app.setup_admin().await;
    let seed = seed_playback_fixtures(&app, &server.db, &admin).await;
    let binary = app.dir.path().join("ffmpeg.sh");
    // Closed fragments are enough for lease tests; a sleeping child makes leaks visible.
    std::fs::write(
        &binary,
        r##"#!/bin/sh
number=0
previous=
for last; do
    if [ "$previous" = "-start_number" ]; then number="$last"; fi
    previous="$last"
done
out_dir=$(dirname "$last")
printf INIT > "$out_dir/init.mp4"
printf SEGMENT > "$out_dir/$number.m4s"
printf '#EXTM3U\n#EXT-X-MAP:URI="init.mp4"\n%s.m4s\n' "$number" > "$last"
exec sleep 30
"##,
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut cfg = mbl_transcode::SessionManagerConfig::new(
        binary,
        camino::Utf8PathBuf::from_path_buf(app.dir.path().join("hls")).unwrap(),
        mbl_platform::PlatformProfile::Generic,
    );
    cfg.inactivity_timeout = Duration::from_millis(250);
    let manager = mbl_transcode::SessionManager::new(cfg).unwrap();
    server.services.stream = server.services.stream.clone().with_hls(manager.clone());
    server.services.playback =
        server.services.playback.clone().with_stream(server.services.stream.clone());
    Fixture {
        playback: server.services.playback.clone(),
        server,
        app,
        manager,
        item: seed.movie_id,
    }
}
fn request(item: i64, hls: bool) -> PlaybackRequest {
    let mut req: PlaybackRequest = serde_json::from_value(json!({
        "itemId": item, "waitForReady": true, "audioOffsetMs": if hls {250} else {0}
    }))
    .unwrap();
    // Network is server-derived and intentionally cannot be supplied through JSON.
    req.network = Some(if hls {
        mbl_service::PlaybackNetwork::Wan
    } else {
        mbl_service::PlaybackNetwork::Lan
    });
    req
}

fn progress(position_ms: u64, state: PlaybackState) -> PlaybackProgressRequest {
    PlaybackProgressRequest { position_ms, state }
}
async fn history(db: &mbl_db::Db) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM watch_history").fetch_one(&db.read).await.unwrap()
}
async fn wait_until_empty(playback: &mbl_service::PlaybackService) {
    tokio::time::timeout(Duration::from_secs(4), async {
        while !playback.list_active(&Actor::admin(UserId(1))).is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn paused_and_buffering_heartbeats_keep_hls_alive_and_foreign_users_cannot_renew() {
    let f = fixture().await;
    let actor = Actor::admin(UserId(1));
    let started = f.playback.start(&actor, request(f.item, true)).await.unwrap();
    let session = f.manager.get_session(&started.session_id).unwrap();
    assert_eq!(f.server.services.remote.bandwidth().active_session_count(), 1);
    let initial = session.last_access().await;
    assert!(
        f.playback
            .progress(&Actor::admin(UserId(2)), &session.id, progress(90000, PlaybackState::Paused))
            .await
            .is_err()
    );
    assert_eq!(session.last_access().await, initial);
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(80)).await;
        f.playback
            .progress(&actor, &session.id, progress(0, PlaybackState::Buffering))
            .await
            .unwrap();
        f.playback.maintain_sessions().await;
        assert_eq!(f.manager.active_count(), 1);
    }
    assert_eq!(history(&f.server.db).await, 0);
    f.playback
        .progress(&actor, &session.id, progress(90000, PlaybackState::Playing))
        .await
        .unwrap();
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(80)).await;
        f.playback
            .progress(&actor, &session.id, progress(90000, PlaybackState::Paused))
            .await
            .unwrap();
        f.playback.maintain_sessions().await;
    }
    assert_eq!(
        session.last_requested_segment().await,
        0,
        "heartbeats must not move the fetch cursor"
    );
    assert!(session.request_segment("0", 0).await.is_ok(), "paused stream still resumes");
    assert!(f.playback.playback_active());
    tokio::time::sleep(Duration::from_millis(300)).await;
    f.playback.maintain_sessions().await;
    assert_eq!(f.manager.active_count(), 0);
    assert!(f.playback.list_active(&actor).is_empty());
    assert_eq!(f.server.services.remote.bandwidth().active_session_count(), 0);
    assert!(!session.dir.exists());
    assert!(session.ffmpeg_pid().await.is_none());
    assert!(session.renew_activity().await.is_err());
    assert!(
        f.playback
            .progress(&actor, &session.id, progress(90000, PlaybackState::Playing))
            .await
            .is_err()
    );
    assert_eq!(history(&f.server.db).await, 1);
    f.playback.maintain_sessions().await;
    assert_eq!(history(&f.server.db).await, 1);
    let replacement = f.playback.start(&actor, request(f.item, true)).await.unwrap();
    assert_eq!(replacement.start_ms, 90000);
    f.playback.shutdown().await;
}
#[tokio::test]
async fn expiry_does_not_overwrite_newer_progress_and_discards_unplayed_candidates() {
    let f = fixture().await;
    let actor = Actor::admin(UserId(1));
    let old = f.playback.start(&actor, request(f.item, true)).await.unwrap();
    f.playback
        .progress(&actor, &old.session_id, progress(80000, PlaybackState::Playing))
        .await
        .unwrap();
    let new = f.playback.start(&actor, request(f.item, false)).await.unwrap();
    f.playback
        .progress(&actor, &new.session_id, progress(140000, PlaybackState::Playing))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    f.playback.maintain_sessions().await;
    assert!(f.playback.list_active(&actor).is_empty());
    assert_eq!(history(&f.server.db).await, 2);
    assert_eq!(
        mbl_db::repo::watch::get(&f.server.db, UserId(1), mbl_core::ItemId(f.item))
            .await
            .unwrap()
            .view_offset_ms,
        140000
    );
    let unplayed = f.playback.start(&actor, request(f.item, true)).await.unwrap();
    f.manager.stop_session(&unplayed.session_id).await; // Simulate quota eviction.
    f.playback.maintain_sessions().await;
    assert_eq!(history(&f.server.db).await, 2);
    assert_eq!(f.server.services.remote.bandwidth().active_session_count(), 0);
}
#[tokio::test]
async fn server_background_lifecycle_reaps_without_new_requests_and_shuts_down() {
    let f = fixture().await;
    let actor = Actor::admin(UserId(1));
    let started = f.playback.start(&actor, request(f.item, true)).await.unwrap();
    let session = f.manager.get_session(&started.session_id).unwrap();
    let stop = CancellationToken::new();
    let (_router, _db) = f.server.start_jobs(stop.clone()).await.unwrap();
    wait_until_empty(&f.playback).await;
    assert!(session.ffmpeg_pid().await.is_none());
    assert!(!session.dir.exists());
    let live = f.playback.start(&actor, request(f.item, true)).await.unwrap();
    let session = f.manager.get_session(&live.session_id).unwrap();
    stop.cancel();
    wait_until_empty(&f.playback).await;
    assert!(session.ffmpeg_pid().await.is_none());
    assert!(!session.dir.exists());
    assert!(f.playback.start(&actor, request(f.item, false)).await.is_err());
    assert_eq!(f.manager.active_count(), 0);
    drop(f.app);
}
#[tokio::test]
async fn http_server_shutdown_awaits_child_cleanup_before_closing_database() {
    let f = fixture().await;
    let actor = Actor::admin(UserId(1));
    let started = f.playback.start(&actor, request(f.item, true)).await.unwrap();
    let session = f.manager.get_session(&started.session_id).unwrap();
    assert!(session.ffmpeg_pid().await.is_some());
    tokio::time::timeout(Duration::from_secs(5), f.server.serve(async {})).await.unwrap().unwrap();
    assert_eq!(f.manager.active_count(), 0);
    assert!(f.playback.list_active(&actor).is_empty());
    assert!(session.ffmpeg_pid().await.is_none());
    assert!(!session.dir.exists());
    assert!(f.playback.start(&actor, request(f.item, false)).await.is_err());
    assert!(session.request_segment("0", 0).await.is_err());
    let gone = f.app.call(Method::GET, "/health", None, None).await;
    assert_eq!(gone.status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
}
