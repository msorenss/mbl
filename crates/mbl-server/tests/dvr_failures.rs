#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_core::{ChannelId, LibraryId, RecordingId, time::now_ms};
use mbl_db::{
    Db,
    repo::{dvr, m6, m9},
};
use serde_json::json;
#[cfg(unix)]
use std::path::Path;
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

async fn fixture(source: &str) -> (TestApp, Db, mbl_service::Services, String, i64, LibraryId) {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let result=app.call(Method::POST,"/api/v1/livetv/tuners/m3u",Some(&admin),Some(json!({"name":"DVR failures","content":format!("#EXTM3U\n#EXTINF:-1 tvg-id=\"capture\",Capture\n{source}\n")}))).await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.body);
    sqlx::query("UPDATE tuners SET tuner_count=1").execute(&db.write).await.unwrap();
    let channel =
        sqlx::query_scalar("SELECT id FROM channels LIMIT 1").fetch_one(&db.read).await.unwrap();
    let root = app.dir.path().join("data/recordings");
    let library = dvr::ensure_library(&db, root.to_str().unwrap(), now_ms()).await.unwrap();
    (app, db, services, admin, channel, library)
}
async fn schedule(
    app: &TestApp,
    db: &Db,
    token: &str,
    channel: i64,
    seconds: i64,
    rating: &str,
) -> RecordingId {
    let now = now_ms();
    m9::batch_upsert_epg_programs(
        db,
        &[m9::EpgProgramInsert {
            channel_id: Some(ChannelId(channel)),
            epg_channel_id: "capture".into(),
            title: "Captured programme".into(),
            description: Some("Guide metadata".into()),
            season_number: None,
            episode_number: None,
            start_time: now - 100,
            end_time: now + seconds * 1000,
            icon_url: None,
            rating: Some(rating.into()),
            genres: vec![],
            is_new: true,
        }],
    )
    .await
    .unwrap();
    let program: i64 = sqlx::query_scalar("SELECT id FROM epg_programs ORDER BY id DESC LIMIT 1")
        .fetch_one(&db.read)
        .await
        .unwrap();
    let result=app.call(Method::POST,"/api/v1/livetv/recordings",Some(token),Some(json!({"channelId":channel,"programId":program,"title":"ignored","startTime":1,"endTime":2}))).await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.body);
    RecordingId(result.body["id"].as_i64().unwrap())
}
fn launch(
    app: &TestApp,
    db: &Db,
    services: &mbl_service::Services,
    live: mbl_service::LiveTvService,
    ffmpeg: PathBuf,
) -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let stop = CancellationToken::new();
    let task = tokio::spawn(mbl_server::recorder::run(
        db.clone(),
        live,
        services.playback.clone(),
        ffmpeg,
        app.dir.path().join("data/recordings"),
        stop.clone(),
    ));
    (stop, task)
}
async fn finished(db: &Db, id: RecordingId) -> m9::RecordingRow {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let row = m9::get_recording(db, id).await.unwrap().unwrap();
            if matches!(row.status.as_str(), "completed" | "partial" | "failed") {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn clean_early_eof_is_playable_partial_with_acl_retry_and_owned_deletion() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel, library) = fixture(&source.url).await;
    let id = schedule(&app, &db, &admin, channel, 120, "G").await;
    let (stop, task) = launch(
        &app,
        &db,
        &services,
        services.livetv.clone(),
        services.stream.ffmpeg_bin().to_path_buf(),
    );
    let row = finished(&db, id).await;
    stop.cancel();
    task.await.unwrap();
    assert_eq!(row.status, "partial", "{:?}", row.error_message);
    assert!((5000..7000).contains(&row.duration_ms.unwrap()));
    assert!(row.error_message.as_deref().unwrap().contains("captured"));
    let item = row.item_id.unwrap();
    let catalogue =
        app.call(Method::GET, &format!("/api/v1/items/{item}"), Some(&admin), None).await;
    assert!(catalogue.body["title"].as_str().unwrap().contains("Incomplete recording"));
    let path = PathBuf::from(row.output_path.unwrap());
    assert!(path.exists());
    let list =
        app.call(Method::GET, "/api/v1/livetv/recordings?status=partial", Some(&admin), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);
    assert_eq!(list.body[0]["itemId"], item);
    let viewer = app.create_user(&admin, "partial-viewer").await;
    let user = services.auth.authenticate(&viewer).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling='G' WHERE id=?")
        .bind(user.0)
        .execute(&db.write)
        .await
        .unwrap();
    m6::grant_library_access(&db, user, library).await.unwrap();
    let detail = app
        .call(Method::GET, &format!("/api/v1/livetv/recordings/{}", id.0), Some(&viewer), None)
        .await;
    assert_eq!(detail.status, StatusCode::OK, "{}", detail.body);
    assert_eq!(detail.body["canDelete"], false);
    assert_eq!(
        app.call(
            Method::DELETE,
            &format!("/api/v1/livetv/recordings/{}", id.0),
            Some(&viewer),
            None
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let play = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&viewer),
            Some(json!({"itemId":item,"profile":"chrome","waitForReady":true})),
        )
        .await;
    assert_eq!(play.status, StatusCode::OK, "{}", play.body);
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", play.body["sessionId"].as_str().unwrap()),
        Some(&viewer),
        None,
    )
    .await;
    m6::set_user_libraries(&db, user, &[]).await.unwrap();
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/livetv/recordings/{}", id.0), Some(&viewer), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let retry=app.call(Method::POST,"/api/v1/livetv/recordings",Some(&admin),Some(json!({"channelId":channel,"programId":row.program_id,"title":"Retry","startTime":1,"endTime":2}))).await;
    assert_eq!(retry.status, StatusCode::OK, "{}", retry.body);
    assert_ne!(retry.body["id"], id.0);
    assert!(path.exists());
    let caller = mbl_mcp::Caller {
        actor: services.auth.authenticate(&admin).await.unwrap().actor,
        scopes: mbl_auth::tokens::ScopeSet::full_for_user(true),
        client: "dvr-test".into(),
        token_id: None,
    };
    let listed = mbl_mcp::execute_tool(
        "list_recordings",
        &json!({"status":"partial"}),
        &caller,
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(serde_json::to_value(listed).unwrap().to_string().contains("partial"));
    let dry = mbl_mcp::execute_tool(
        "cancel_recording",
        &json!({"recording_id":id.0,"dry_run":true}),
        &caller,
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(dry.structured_content.unwrap()["deletesSavedFile"], true);
    assert!(path.exists());
    assert_eq!(
        app.call(
            Method::DELETE,
            &format!("/api/v1/livetv/recordings/{}", id.0),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::OK
    );
    assert!(!path.exists());
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item}"), Some(&admin), None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn fast_sources_covering_the_requested_media_window_still_complete() {
    let source = common::live_source_with_duration(30).await;
    let (app, db, services, admin, channel, _) = fixture(&source.url).await;
    let id = schedule(&app, &db, &admin, channel, 20, "G").await;
    let (stop, task) = launch(
        &app,
        &db,
        &services,
        services.livetv.clone(),
        services.stream.ffmpeg_bin().to_path_buf(),
    );
    let row = finished(&db, id).await;
    stop.cancel();
    task.await.unwrap();
    assert_eq!(row.status, "completed", "{:?}", row.error_message);
    assert!(row.duration_ms.unwrap() > 19000);
    assert!(row.item_id.is_some());
    assert!(row.error_message.is_none());
}
#[cfg(unix)]
fn script(app: &TestApp, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = app.dir.path().join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}
#[cfg(unix)]
async fn wait_for(path: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[cfg(unix)]
#[tokio::test]
async fn encoder_errors_salvage_only_real_media_and_reject_empty_or_corrupt_output() {
    let source = common::live_source().await;
    for valid in [Some(true), Some(false), None] {
        let (app, db, services, admin, channel, _) = fixture(&source.url).await;
        let input = app.dir.path().join("input.ts");
        tokio::fs::write(&input, &source.bytes).await.unwrap();
        let write = match valid {
            Some(true) => format!("cp '{}' \"$last\"", input.display()),
            Some(false) => "printf garbage > \"$last\"".into(),
            None => String::new(),
        };
        let ffmpeg = script(&app, "error.sh", &format!("for last; do true; done\n{write}\nexit 1"));
        let id = schedule(&app, &db, &admin, channel, 120, "G").await;
        let (stop, task) = launch(&app, &db, &services, services.livetv.clone(), ffmpeg);
        let row = finished(&db, id).await;
        stop.cancel();
        task.await.unwrap();
        if valid == Some(true) {
            assert_eq!(row.status, "partial", "{:?}", row.error_message);
            assert!(row.item_id.is_some());
            assert!(row.error_message.unwrap().contains("FFmpeg exited"));
            assert!(row.duration_ms.unwrap() > 5000);
        } else {
            assert_eq!(row.status, "failed");
            assert!(row.item_id.is_none());
        }
        assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
    }
}
#[cfg(unix)]
#[tokio::test]
async fn profile_changes_during_probe_do_not_publish_stale_ratings() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel, library) = fixture(&source.url).await;
    let viewer = app.create_user(&admin, "capture-owner").await;
    let user = services.auth.authenticate(&viewer).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling='G' WHERE id=?")
        .bind(user.0)
        .execute(&db.write)
        .await
        .unwrap();
    m6::grant_library_access(&db, user, library).await.unwrap();
    let id = schedule(&app, &db, &viewer, channel, 120, "G").await;
    let barrier = app.dir.path().join("probe-ready");
    let resume = app.dir.path().join("probe-resume");
    let mut tools = mbl_probe::FfTools::locate("", "").await.unwrap();
    let wrapper = script(
        &app,
        "probe.sh",
        &format!(
            "touch '{}'\nwhile [ ! -f '{}' ]; do sleep 0.02; done\nexec '{}' \"$@\"",
            barrier.display(),
            resume.display(),
            tools.ffprobe.display()
        ),
    );
    tools.ffprobe = wrapper;
    let live = services
        .livetv
        .clone()
        .with_recordings(app.dir.path().join("data/recordings"), Some(tools));
    let (stop, task) =
        launch(&app, &db, &services, live, services.stream.ffmpeg_bin().to_path_buf());
    wait_for(&barrier).await;
    sqlx::query("UPDATE epg_programs SET rating='R'").execute(&db.write).await.unwrap();
    tokio::fs::write(&resume, b"resume").await.unwrap();
    let row = finished(&db, id).await;
    stop.cancel();
    task.await.unwrap();
    assert_eq!(row.status, "failed", "{:?}", row.error_message);
    assert!(row.item_id.is_none());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items").fetch_one(&db.read).await.unwrap();
    assert_eq!(count, 0);
}
#[cfg(unix)]
#[tokio::test]
async fn shutdown_never_publishes_an_interrupted_capture_as_a_source_error_partial() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel, _) = fixture(&source.url).await;
    let input = app.dir.path().join("input.ts");
    tokio::fs::write(&input, &source.bytes).await.unwrap();
    let ready = app.dir.path().join("capture-ready");
    let ffmpeg = script(
        &app,
        "shutdown.sh",
        &format!(
            "for last; do true; done\ncp '{}' \"$last\"\ntouch '{}'\nexec sleep 30",
            input.display(),
            ready.display()
        ),
    );
    let id = schedule(&app, &db, &admin, channel, 120, "G").await;
    let (stop, task) = launch(&app, &db, &services, services.livetv.clone(), ffmpeg);
    wait_for(&ready).await;
    stop.cancel();
    task.await.unwrap();
    let row = m9::get_recording(&db, id).await.unwrap().unwrap();
    assert_eq!(row.status, "failed");
    assert!(row.item_id.is_none());
    assert!(row.error_message.unwrap().contains("shutdown"));
}

#[cfg(unix)]
#[tokio::test]
async fn stalled_capture_stops_at_the_approved_end_and_preserves_only_a_marked_partial() {
    let source = common::live_source_with_duration(1).await;
    let (app, db, services, admin, channel, _) = fixture(&source.url).await;
    let input = app.dir.path().join("input.ts");
    tokio::fs::write(&input, &source.bytes).await.unwrap();
    let ffmpeg = script(
        &app,
        "deadline.sh",
        &format!("for last; do true; done\ncp '{}' \"$last\"\nexec sleep 30", input.display()),
    );
    let id = schedule(&app, &db, &admin, channel, 3, "G").await;
    let end = m9::get_recording(&db, id).await.unwrap().unwrap().end_time;
    let (stop, task) = launch(&app, &db, &services, services.livetv.clone(), ffmpeg);
    let row = tokio::time::timeout(Duration::from_secs(7), finished(&db, id)).await.unwrap();
    stop.cancel();
    task.await.unwrap();
    assert!(now_ms() < end + 3000, "must not wait for the former 15-second grace period");
    assert_eq!(row.status, "partial", "{:?}", row.error_message);
    assert!(row.item_id.is_some());
    assert!(row.duration_ms.unwrap() < 2000);
    let lease = services
        .livetv
        .reserve_recording(RecordingId(999), ChannelId(channel), now_ms() + 1000)
        .await
        .unwrap();
    lease.release().await;
}
