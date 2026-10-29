#![allow(clippy::unwrap_used)]
mod common;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::TestApp;
use http_body_util::BodyExt;
use mbl_core::{ChannelId, RecordingId};
use mbl_jobs::ActivityProbe;
use serde_json::json;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

async fn fixture(source: &str) -> (TestApp, mbl_db::Db, mbl_service::Services, String, i64) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mbl_server::Config::default();
    config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
    config.transcode.max_sessions = 1;
    let server = mbl_server::build(config).await.unwrap();
    let app = TestApp { router: server.router, dir };
    let token = app.setup_admin().await;
    let imported = app.call(Method::POST, "/api/v1/livetv/tuners/m3u", Some(&token), Some(json!({"name":"Test tuner", "content":format!("#EXTM3U\n#EXTINF:-1,Local test\n{source}\n")}))).await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.body);
    sqlx::query("UPDATE tuners SET tuner_count=1").execute(&server.db.write).await.unwrap();
    let channels = app.call(Method::GET, "/api/v1/livetv/channels", Some(&token), None).await;
    (app, server.db, server.services, token, channels.body[0]["id"].as_i64().unwrap())
}
async fn start(app: &TestApp, token: &str, channel: i64) -> common::Res {
    app.call(Method::GET, &format!("/api/v1/livetv/stream/{channel}"), Some(token), None).await
}
async fn release(app: &TestApp, token: &str, channel: i64, id: &str) {
    let result = app
        .call(
            Method::POST,
            &format!("/api/v1/livetv/stream/{channel}/release?sessionId={id}"),
            Some(token),
            None,
        )
        .await;
    assert_eq!(result.status, StatusCode::OK);
}
async fn read(app: &TestApp, url: &str) -> Vec<u8> {
    let response =
        app.router.clone().oneshot(Request::get(url).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body().collect().await.unwrap().to_bytes().to_vec()
}
#[tokio::test]
async fn live_relay_serves_real_browser_media_and_revokes_capabilities_on_release() {
    let source = common::live_source().await;
    let (app, _db, services, token, channel) = fixture(&source.url).await;
    let stream = start(&app, &token, channel).await;
    assert_eq!(stream.status, StatusCode::OK, "{}", stream.body);
    assert_eq!(
        stream.headers["cache-control"], "private, no-store",
        "a renewal must reach the server, never a cache"
    );
    assert_eq!(stream.body["targetFormat"], "hls");
    assert_eq!(stream.body["isDeinterlacingRequired"], false);
    let url = stream.body["streamUrl"].as_str().unwrap();
    assert!(url.starts_with("/api/v1/stream/live/"));
    assert!(!url.contains(&source.url));
    let playlist = String::from_utf8(read(&app, url).await).unwrap();
    assert!(playlist.contains("#EXTINF:"));
    assert!(services.playback.playback_active(), "idle analysis must yield to live encoding");
    let head =
        app.router.clone().oneshot(Request::head(url).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(head.headers()["content-length"].to_str().unwrap(), playlist.len().to_string());
    assert!(head.into_body().collect().await.unwrap().to_bytes().is_empty());
    let segment = playlist.lines().find(|s| s.starts_with("seg_")).unwrap();
    let base = url.trim_end_matches("index.m3u8");
    let mut media = read(&app, &format!("{base}init.mp4")).await;
    media.extend(read(&app, &format!("{base}{segment}")).await);
    let path = app.dir.path().join("fragment.mp4");
    tokio::fs::write(&path, media).await.unwrap();
    let probe = tokio::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_name,field_order", "-of", "json"])
        .arg(&path)
        .output()
        .await
        .unwrap();
    assert!(probe.status.success());
    let info: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert!(
        info["streams"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["codec_name"] == "h264" && s["field_order"] == "progressive")
    );
    assert!(info["streams"].as_array().unwrap().iter().any(|s| s["codec_name"] == "aac"));
    let range = app
        .router
        .clone()
        .oneshot(
            Request::get(format!("{base}{segment}"))
                .header("Range", "bytes=0-15")
                .header("Origin", "https://receiver.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(range.headers()["content-length"], "16");
    assert_eq!(range.headers()["cache-control"], "private, no-store");
    assert_eq!(range.headers()["access-control-allow-origin"], "https://receiver.example");
    // Complete the range body before closing the lease (also releases Windows file handles).
    assert_eq!(range.into_body().collect().await.unwrap().to_bytes().len(), 16);
    let id = stream.body["sessionId"].as_str().unwrap();
    assert!(services.livetv.live_file(id, "../init.mp4").await.is_err());
    assert!(services.livetv.live_file(id, "index.m3u8.tmp").await.is_err());
    assert!(services.livetv.live_file("not-a-lease", "init.mp4").await.is_err());
    assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 1);
    release(&app, &token, channel, id).await;
    assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
    assert_eq!(app.call(Method::GET, url, None, None).await.status, StatusCode::NOT_FOUND);
    let directory = services.stream.hls_manager().unwrap().config().transcode_dir.join("live");
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
}
#[tokio::test]
async fn live_and_vod_share_encoder_capacity_and_failed_admission_does_not_leak_tuners() {
    let source = common::live_source().await;
    let (app, db, services, token, channel) = fixture(&source.url).await;
    let seeded = common::seed_playback_fixtures(&app, &db, &token).await;
    let live = start(&app, &token, channel).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.body);
    let vod = json!({"itemId":seeded.movie_id,"audioOffsetMs":250,"maxBitrate":1000000});
    let blocked = app.call(Method::POST, "/api/v1/playback", Some(&token), Some(vod.clone())).await;
    assert_eq!(blocked.status, StatusCode::SERVICE_UNAVAILABLE);
    release(&app, &token, channel, live.body["sessionId"].as_str().unwrap()).await;
    let playing = app.call(Method::POST, "/api/v1/playback", Some(&token), Some(vod)).await;
    assert_eq!(playing.status, StatusCode::OK, "{}", playing.body);
    let blocked = start(&app, &token, channel).await;
    assert_eq!(blocked.status, StatusCode::SERVICE_UNAVAILABLE);
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", playing.body["sessionId"].as_str().unwrap()),
        Some(&token),
        None,
    )
    .await;
    tokio::task::yield_now().await;
    let next = start(&app, &token, channel).await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.body);
    release(&app, &token, channel, next.body["sessionId"].as_str().unwrap()).await;
    assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
}
#[tokio::test]
async fn live_and_dvr_use_the_same_tuner_booking() {
    let source = common::live_source().await;
    let (app, _db, services, token, channel) = fixture(&source.url).await;
    let lease = services
        .livetv
        .reserve_recording(RecordingId(99), ChannelId(channel), mbl_core::time::now_ms() + 60000)
        .await
        .unwrap();
    assert_eq!(start(&app, &token, channel).await.status, StatusCode::CONFLICT);
    lease.release().await;
    let live = start(&app, &token, channel).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.body);
    assert!(
        services
            .livetv
            .reserve_recording(
                RecordingId(100),
                ChannelId(channel),
                mbl_core::time::now_ms() + 60000
            )
            .await
            .is_err()
    );
    release(&app, &token, channel, live.body["sessionId"].as_str().unwrap()).await;
    let lease = services
        .livetv
        .reserve_recording(RecordingId(101), ChannelId(channel), mbl_core::time::now_ms() + 60000)
        .await
        .unwrap();
    lease.release().await;
}
#[tokio::test]
async fn dvr_executes_real_media_and_reports_live_tuner_conflicts() {
    // A genuine five-minute file exercises the normal >60 s resume threshold.
    let source = common::live_source_with_duration(300).await;
    let (app, db, services, token, channel) = fixture(&source.url).await;
    let now = mbl_core::time::now_ms();
    let id = mbl_db::repo::m9::schedule_recording(
        &db,
        None,
        ChannelId(channel),
        None,
        "Synthetic DVR",
        now - 100,
        now + 300000,
    )
    .await
    .unwrap();
    let output = app.dir.path().join("data/recordings");
    let stop = CancellationToken::new();
    let task = tokio::spawn(mbl_server::recorder::run(
        db.clone(),
        services.livetv.clone(),
        services.stream.ffmpeg_bin().to_path_buf(),
        output.clone(),
        stop.clone(),
    ));
    let recording = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let row = mbl_db::repo::m9::get_recording(&db, id).await.unwrap().unwrap();
            if row.status == "completed" || row.status == "failed" {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(recording.status, "completed", "{:?}", recording.error_message);
    assert!(recording.size_bytes.unwrap() > 0);
    assert!(output.join(format!("{}.ts", id.0)).exists());
    let tools = mbl_probe::FfTools::locate("", "").await.unwrap();
    let info =
        tools.probe(&output.join(format!("{}.ts", id.0)), Duration::from_secs(10)).await.unwrap();
    assert_eq!(recording.duration_ms, Some(info.duration_ms as i64));
    assert!(info.duration_ms > 2000, "media time must not be the fast copy job's wall time");
    let item = recording.item_id.unwrap();
    let details = app
        .call(Method::GET, &format!("/api/v1/livetv/recordings/{}", id.0), Some(&token), None)
        .await;
    assert_eq!(details.body["itemId"], item);
    assert!(details.body.get("outputPath").is_none());
    let play = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&token),
            Some(json!({"itemId":item,"profile":"chrome","waitForReady":true})),
        )
        .await;
    assert_eq!(play.status, StatusCode::OK, "{}", play.body);
    let session = services.stream.hls_session(play.body["sessionId"].as_str().unwrap()).unwrap();
    session.request_segment("0", 0).await.unwrap();
    let fragment = session.dir.join("0/init.mp4");
    assert!(fragment.exists(), "DVR must use the real browser HLS pipeline");
    let progress = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{}/progress", play.body["sessionId"].as_str().unwrap()),
            Some(&token),
            Some(json!({"positionMs":61000,"state":"paused"})),
        )
        .await;
    assert_eq!(progress.status, StatusCode::NO_CONTENT, "{}", progress.body);
    let stopped = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{}/stop", play.body["sessionId"].as_str().unwrap()),
            Some(&token),
            None,
        )
        .await;
    assert_eq!(stopped.status, StatusCode::NO_CONTENT);
    let offset: i64 = sqlx::query_scalar("SELECT view_offset_ms FROM watch_state WHERE item_id=?")
        .bind(item)
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(offset, 61000);
    let resume = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&token),
            Some(json!({"itemId":item,"profile":"chrome","waitForReady":true})),
        )
        .await;
    assert_eq!(resume.status, StatusCode::OK, "{}", resume.body);
    assert_eq!(resume.body["startMs"], 61000);
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", resume.body["sessionId"].as_str().unwrap()),
        Some(&token),
        None,
    )
    .await;
    let history: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM watch_history WHERE item_id=?")
        .bind(item)
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(history, 1);
    let live = start(&app, &token, channel).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.body);
    let now = mbl_core::time::now_ms();
    let conflict = mbl_db::repo::m9::schedule_recording(
        &db,
        None,
        ChannelId(channel),
        None,
        "Conflict",
        now - 1,
        now + 10000,
    )
    .await
    .unwrap();
    let row = tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            let row = mbl_db::repo::m9::get_recording(&db, conflict).await.unwrap().unwrap();
            if row.status == "failed" {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert!(row.error_message.unwrap().contains("tuners are in use"));
    assert!(!output.join(format!("{}.ts", conflict.0)).exists());
    release(&app, &token, channel, live.body["sessionId"].as_str().unwrap()).await;
    stop.cancel();
    task.await.unwrap();
}

async fn legacy_recording(
    app: &TestApp,
    db: &mbl_db::Db,
    channel: i64,
    source: &axum::body::Bytes,
    title: &str,
    program: Option<mbl_core::ProgramId>,
) -> (RecordingId, std::path::PathBuf) {
    let now = mbl_core::time::now_ms();
    let id = mbl_db::repo::m9::schedule_recording(
        db,
        None,
        ChannelId(channel),
        program,
        title,
        now,
        now + 60000,
    )
    .await
    .unwrap();
    let root = app.dir.path().join("data/recordings");
    tokio::fs::create_dir_all(&root).await.unwrap();
    let path = root.join(format!("{}.ts", id.0));
    tokio::fs::write(&path, source).await.unwrap();
    mbl_db::repo::m9::update_recording_status(
        db,
        id,
        "completed",
        Some(path.to_str().unwrap()),
        None,
        Some(1),
        None,
    )
    .await
    .unwrap();
    (id, path)
}

#[tokio::test]
async fn dvr_recovers_legacy_files_once_enforces_acl_rating_and_deletes_the_saved_media() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel) = fixture(&source.url).await;
    let program: i64 = sqlx::query_scalar("INSERT INTO epg_programs(channel_id,epg_channel_id,title,description,start_time,end_time,rating) VALUES (?,'test','Guide title','Captured guide description',1,2,'G') RETURNING id").bind(channel).fetch_one(&db.write).await.unwrap();
    let (id, path) = legacy_recording(
        &app,
        &db,
        channel,
        &source.bytes,
        "Legacy DVR",
        Some(mbl_core::ProgramId(program)),
    )
    .await;
    sqlx::query("DELETE FROM epg_programs").execute(&db.write).await.unwrap();
    services.livetv.reconcile_recordings().await;
    let item = mbl_db::repo::m9::get_recording(&db, id).await.unwrap().unwrap().item_id.unwrap();
    let again = services.livetv.publish_recording(id).await.unwrap();
    assert_eq!(again.0, item);
    let (library, duration, probe): (i64, i64, String) = sqlx::query_as("SELECT i.library_id,i.duration_ms,p.probe FROM media_items i JOIN media_versions v ON v.item_id=i.id JOIN media_parts p ON p.version_id=v.id WHERE i.id=?").bind(item).fetch_one(&db.read).await.unwrap();
    let info: mbl_probe::MediaInfo = serde_json::from_str(&probe).unwrap();
    assert_eq!(duration, info.duration_ms as i64);
    assert!(duration > 5000);
    let (rating, summary): (String, String) =
        sqlx::query_as("SELECT content_rating,summary FROM media_items WHERE id=?")
            .bind(item)
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert_eq!(rating, "G");
    assert_eq!(summary, "Captured guide description");
    tokio::fs::write(path.parent().unwrap().join("partial.ts"), b"not finalized").await.unwrap();
    let scanner =
        mbl_scanner::Scanner::new(db.clone(), None, mbl_core::events::EventBus::default(), None, 1);
    scanner.scan(mbl_core::LibraryId(library), &CancellationToken::new(), |_| {}).await.unwrap();
    let parts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_parts").fetch_one(&db.read).await.unwrap();
    assert_eq!(parts, 1, "general scanning must not import partial DVR files");
    assert_eq!(info.primary_video().unwrap().codec, "mpeg2video");
    assert!(info.streams.iter().any(|s| s.codec == "ac3"));
    let bob = app.create_user(&admin, "dvr-bob").await;
    let user: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username='dvr-bob'")
        .fetch_one(&db.read)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_managed=1 WHERE id=?")
        .bind(user)
        .execute(&db.write)
        .await
        .unwrap();
    for allowed in [false, true] {
        if allowed {
            sqlx::query(
                "INSERT INTO user_library_access(user_id,library_id,created_at) VALUES (?,?,0)",
            )
            .bind(user)
            .bind(library)
            .execute(&db.write)
            .await
            .unwrap();
        }
        if !allowed {
            let denied = app
                .call(Method::POST, "/api/v1/playback", Some(&bob), Some(json!({"itemId":item})))
                .await;
            assert_eq!(denied.status, StatusCode::NOT_FOUND);
        }
        let list = app.call(Method::GET, "/api/v1/livetv/recordings", Some(&bob), None).await;
        assert_eq!(list.body.as_array().unwrap().len(), usize::from(allowed));
        let get = app
            .call(Method::GET, &format!("/api/v1/livetv/recordings/{}", id.0), Some(&bob), None)
            .await;
        assert_eq!(get.status, if allowed { StatusCode::OK } else { StatusCode::NOT_FOUND });
    }
    sqlx::query("UPDATE media_items SET content_rating='R' WHERE id=?")
        .bind(item)
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET content_rating_ceiling='G' WHERE id=?")
        .bind(user)
        .execute(&db.write)
        .await
        .unwrap();
    assert!(
        app.call(Method::GET, "/api/v1/livetv/recordings", Some(&bob), None)
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    for (method, url, body) in [
        (Method::POST, "/api/v1/playback".to_owned(), Some(json!({"itemId":item}))),
        (Method::DELETE, format!("/api/v1/livetv/recordings/{}", id.0), None),
    ] {
        assert_eq!(app.call(method, &url, Some(&bob), body).await.status, StatusCode::NOT_FOUND);
    }
    assert!(path.exists());
    let deleted = app
        .call(Method::DELETE, &format!("/api/v1/livetv/recordings/{}", id.0), Some(&admin), None)
        .await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.body);
    assert!(!path.exists());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE id=?")
        .bind(item)
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(mbl_db::repo::m9::get_recording(&db, id).await.unwrap().is_none());
}

#[tokio::test]
async fn dvr_rejects_external_paths_corrupt_files_and_symlinks_without_publishing_them() {
    let source = common::live_source().await;
    let (app, db, services, _admin, channel) = fixture(&source.url).await;
    let (id, path) = legacy_recording(&app, &db, channel, &source.bytes, "Unsafe path", None).await;
    let external = app.dir.path().join("do-not-delete.ts");
    tokio::fs::copy(&path, &external).await.unwrap();
    sqlx::query("UPDATE recordings SET output_path=? WHERE id=?")
        .bind(external.to_str().unwrap())
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    assert!(services.livetv.publish_recording(id).await.is_err());
    assert!(services.livetv.cancel_recording(id).await.is_err());
    assert!(external.exists());
    sqlx::query("UPDATE recordings SET output_path=? WHERE id=?")
        .bind(path.to_str().unwrap())
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    tokio::fs::write(&path, b"corrupted transport stream").await.unwrap();
    assert!(services.livetv.publish_recording(id).await.is_err());
    #[cfg(unix)]
    {
        tokio::fs::remove_file(&path).await.unwrap();
        std::os::unix::fs::symlink(&external, &path).unwrap();
        assert!(services.livetv.publish_recording(id).await.is_err());
        assert!(services.livetv.cancel_recording(id).await.is_err());
        assert!(external.exists());
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items").fetch_one(&db.read).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn dvr_publication_is_idempotent_and_cancellation_during_probing_leaves_no_media() {
    let source = common::live_source().await;
    let (app, db, services, _admin, channel) = fixture(&source.url).await;
    let (id, path) =
        legacy_recording(&app, &db, channel, &source.bytes, "Concurrent DVR", None).await;
    let (first, second) =
        tokio::join!(services.livetv.publish_recording(id), services.livetv.publish_recording(id));
    assert_eq!(first.unwrap(), second.unwrap());
    assert!(services.livetv.cancel_recording(id).await.unwrap());
    assert!(!path.exists());
    let (id, path) =
        legacy_recording(&app, &db, channel, &source.bytes, "Cancelled publication", None).await;
    let (_publication, deletion) =
        tokio::join!(services.livetv.publish_recording(id), services.livetv.cancel_recording(id));
    assert!(deletion.unwrap());
    assert!(!path.exists());
    assert!(mbl_db::repo::m9::get_recording(&db, id).await.unwrap().is_none());
    let items: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items").fetch_one(&db.read).await.unwrap();
    assert_eq!(items, 0);
}
#[tokio::test]
async fn abandoned_live_leases_expire_without_media_requests_renewing_them() {
    let source = common::live_source().await;
    let (app, _db, services, token, channel) = fixture(&source.url).await;
    let live = start(&app, &token, channel).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.body);
    let id = live.body["sessionId"].as_str().unwrap();
    let path = services.livetv.live_file(id, "init.mp4").await.unwrap();
    let expires = live.body["expiresAt"].as_i64().unwrap();
    tokio::time::sleep(Duration::from_millis(
        (expires - mbl_core::time::now_ms()).max(0) as u64 + 20,
    ))
    .await;
    assert!(services.livetv.live_file(id, "init.mp4").await.is_err());
    services.livetv.maintain().await;
    assert!(!path.exists());
    assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
}

#[tokio::test]
async fn deleting_a_channel_revokes_its_live_encoder_and_cache() {
    let source = common::live_source().await;
    let (app, _db, services, token, channel) = fixture(&source.url).await;
    let live = start(&app, &token, channel).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.body);
    let path = services
        .livetv
        .live_file(live.body["sessionId"].as_str().unwrap(), "init.mp4")
        .await
        .unwrap();
    assert_eq!(
        app.call(Method::DELETE, &format!("/api/v1/livetv/channels/{channel}"), Some(&token), None)
            .await
            .status,
        StatusCode::OK
    );
    assert!(!path.exists());
    assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
    assert_eq!(
        app.call(Method::GET, live.body["streamUrl"].as_str().unwrap(), None, None).await.status,
        StatusCode::NOT_FOUND
    );
}
#[cfg(unix)]
#[tokio::test]
async fn cancelling_preparation_and_shutdown_leave_no_live_child_slot_or_directory() {
    use std::os::unix::fs::PermissionsExt;
    let (app, _db, services, token, channel) = fixture("http://127.0.0.1:9/unreachable").await;
    let binary = app.dir.path().join("stall.sh");
    std::fs::write(&binary, "#!/bin/sh\nfor last; do true; done\nout_dir=$(dirname \"$last\")\necho $$ > \"$out_dir/pid\"\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let service =
        services.livetv.clone().with_stream(services.stream.clone().with_ffmpeg_bin(binary));
    let owner = services.auth.authenticate(&token).await.unwrap().session_id;
    let manager = services.stream.hls_manager().unwrap();
    let root = manager.config().transcode_dir.join("live");
    for shutdown in [false, true] {
        let pending = service.clone();
        let task = tokio::spawn(async move {
            pending.resolve_live_stream(owner, ChannelId(channel), None).await
        });
        let pid = tokio::time::timeout(Duration::from_secs(3), async {
            'found: loop {
                if let Ok(entries) = std::fs::read_dir(&root) {
                    for entry in entries.flatten() {
                        if let Ok(pid) = std::fs::read_to_string(entry.path().join("pid")) {
                            break 'found pid;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        // The PID file is only a synchronization point; shutdown must kill and reap the actual child.
        let pid = pid.trim().parse::<u32>().unwrap();
        if shutdown {
            service.shutdown().await;
            assert!(task.await.unwrap().is_err());
        } else {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while manager.active_video_transcode_count() != 0
                || std::fs::read_dir(&root).unwrap().count() != 0
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        #[cfg(target_os = "linux")]
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        #[cfg(not(target_os = "linux"))]
        let _ = pid;
    }
    assert!(
        service
            .reserve_recording(
                RecordingId(55),
                ChannelId(channel),
                mbl_core::time::now_ms() + 10000
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn upstream_failure_releases_live_capacity_and_tuner_reservation() {
    let source = common::live_source().await;
    let (app, _db, services, token, channel) =
        fixture(&source.url.replace("/live.ts", "/missing")).await;
    let failed = start(&app, &token, channel).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE, "{}", failed.body);
    assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
    let lease = services
        .livetv
        .reserve_recording(RecordingId(55), ChannelId(channel), mbl_core::time::now_ms() + 60000)
        .await
        .unwrap();
    lease.release().await;
}
#[tokio::test]
async fn live_cache_obeys_the_shared_temporary_disk_quota() {
    let source = common::live_source().await;
    let (app, _db, services, token, channel) = fixture(&source.url).await;
    let mut config = mbl_transcode::SessionManagerConfig::new(
        services.stream.ffmpeg_bin().to_path_buf(),
        camino::Utf8PathBuf::from_path_buf(app.dir.path().join("quota")).unwrap(),
        mbl_platform::PlatformProfile::Generic,
    );
    config.temp_quota_bytes = 1;
    let manager = mbl_transcode::SessionManager::new(config).unwrap();
    let service =
        services.livetv.clone().with_stream(services.stream.clone().with_hls(manager.clone()));
    let owner = services.auth.authenticate(&token).await.unwrap().session_id;
    let live = service.resolve_live_stream(owner, ChannelId(channel), None).await.unwrap();
    let path = service.live_file(&live.session_id, "init.mp4").await.unwrap();
    assert!(path.exists());
    service.maintain().await;
    assert!(!path.exists());
    assert_eq!(manager.active_video_transcode_count(), 0);
    assert!(service.live_file(&live.session_id, "init.mp4").await.is_err());
}
