#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_core::{Actor, ChannelId, LibraryId, RecordingId, time::now_ms};
use mbl_db::{
    Db,
    repo::{m6, m9},
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

async fn fixture(
    source: &str,
    reserve_mb: u64,
    max_mb: Option<u64>,
) -> (TestApp, Db, mbl_service::Services, String, i64) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mbl_server::Config::default();
    config.paths.data_dir = dir.path().join("data").to_str().unwrap().into();
    config.dvr.min_free_mb = reserve_mb;
    config.dvr.max_recording_mb = max_mb;
    let server = mbl_server::build(config).await.unwrap();
    let app = TestApp { router: server.router, dir };
    let admin = app.setup_admin().await;
    let imported=app.call(Method::POST,"/api/v1/livetv/tuners/m3u",Some(&admin),Some(json!({"name":"DVR plan","content":format!("#EXTM3U\n#EXTINF:-1 tvg-id=\"plans\",Plans\n{source}\n")}))).await;
    assert_eq!(imported.status, StatusCode::OK);
    sqlx::query("UPDATE tuners SET tuner_count=1").execute(&server.db.write).await.unwrap();
    let channel = sqlx::query_scalar("SELECT id FROM channels LIMIT 1")
        .fetch_one(&server.db.read)
        .await
        .unwrap();
    (app, server.db, server.services, admin, channel)
}
async fn programme(db: &Db, ch: i64, title: &str, start: i64, end: i64) -> i64 {
    m9::batch_upsert_epg_programs(
        db,
        &[m9::EpgProgramInsert {
            channel_id: Some(ChannelId(ch)),
            epg_channel_id: "plans".into(),
            title: title.into(),
            description: None,
            season_number: Some(1),
            episode_number: Some(1),
            start_time: start,
            end_time: end,
            icon_url: None,
            rating: Some("G".into()),
            genres: vec![],
            is_new: true,
        }],
    )
    .await
    .unwrap();
    sqlx::query_scalar("SELECT id FROM epg_programs WHERE start_time=?")
        .bind(start)
        .fetch_one(&db.read)
        .await
        .unwrap()
}
fn request(ch: i64, program: i64) -> Value {
    json!({"channelId":ch,"programId":program,"title":"Spoofed","startTime":1,"endTime":2,"startPaddingSecs":2,"endPaddingSecs":3})
}
async fn count(db: &Db, table: &str) -> i64 {
    let query = match table {
        "libraries" => "SELECT COUNT(*) FROM libraries",
        "recordings" => "SELECT COUNT(*) FROM recordings",
        "jobs" => "SELECT COUNT(*) FROM jobs",
        _ => panic!("unexpected test table"),
    };
    sqlx::query_scalar(query).fetch_one(&db.read).await.unwrap()
}
fn caller(actor: Actor) -> mbl_mcp::Caller {
    mbl_mcp::Caller {
        actor,
        scopes: mbl_auth::tokens::ScopeSet::full_for_user(true),
        client: "plans-test".into(),
        token_id: None,
    }
}

#[tokio::test]
async fn preview_and_mcp_use_canonical_guide_data_without_creating_library_or_jobs() {
    let (app, db, services, admin, ch) = fixture("http://127.0.0.1:1/unused", 512, None).await;
    let starts = now_ms() + 3600000;
    let ends = starts + 60000;
    let program = programme(&db, ch, "Canonical programme", starts, ends).await;
    let body = request(ch, program);
    let root = app.dir.path().join("data/recordings");
    assert!(!root.exists());
    assert_eq!(count(&db, "libraries").await, 0);
    let plan = app
        .call(Method::POST, "/api/v1/livetv/recordings/preview", Some(&admin), Some(body.clone()))
        .await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.body);
    assert_eq!(plan.body["title"], "Canonical programme");
    assert_eq!(plan.body["startTime"], starts - 2000);
    assert_eq!(plan.body["endTime"], ends + 3000);
    assert!(plan.body["libraryId"].is_null());
    assert!(plan.body["storage"]["availableBytes"].as_u64().unwrap() > 0);
    let actor = services.auth.authenticate(&admin).await.unwrap().actor;
    let args = json!({"channel_id":ch,"program_id":program,"title":"Wrong title","start_time":1,"end_time":2,"start_padding_secs":2,"end_padding_secs":3,"dry_run":true});
    let dry = mbl_mcp::execute_tool(
        "schedule_recording",
        &args,
        &caller(actor.clone()),
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    let dry = dry.structured_content.unwrap();
    assert_eq!(dry["title"], plan.body["title"]);
    assert_eq!(dry["startTime"], plan.body["startTime"]);
    assert_eq!(dry["dryRun"], true);
    for table in ["libraries", "recordings", "jobs"] {
        assert_eq!(count(&db, table).await, 0, "preview wrote {table}");
    }
    assert!(!root.exists());
    let unused =
        services.livetv.reserve_recording(RecordingId(999), ChannelId(ch), ends).await.unwrap();
    unused.release().await;
    let invalid = mbl_mcp::execute_tool(
        "schedule_recording",
        &json!({"channel_id":ch,"title":"Expired","start_time":1,"end_time":2,"dry_run":true}),
        &caller(actor),
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(invalid.is_error);
    assert_eq!(count(&db, "libraries").await, 0);
    let scheduled =
        app.call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(body)).await;
    assert_eq!(scheduled.status, StatusCode::OK, "{}", scheduled.body);
    assert_eq!(scheduled.body["title"], plan.body["title"]);
    assert_eq!(scheduled.body["startTime"], plan.body["startTime"]);
    assert_eq!(scheduled.body["endTime"], plan.body["endTime"]);
    assert_eq!(count(&db, "libraries").await, 1);
    assert_eq!(count(&db, "recordings").await, 1);
}

#[tokio::test]
async fn retry_preserves_managed_owner_rule_and_retention_and_rechecks_policy() {
    let source = common::live_source().await;
    let (app, db, services, admin, ch) = fixture(&source.url, 512, None).await;
    app.call(
        Method::POST,
        "/api/v1/livetv/rules",
        Some(&admin),
        Some(json!({"title":"Seed library"})),
    )
    .await;
    let lib: i64 = sqlx::query_scalar("SELECT library_id FROM dvr_libraries")
        .fetch_one(&db.read)
        .await
        .unwrap();
    let child = app.create_user(&admin, "retry-child").await;
    let user = services.auth.authenticate(&child).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling='G' WHERE id=?")
        .bind(user.0)
        .execute(&db.write)
        .await
        .unwrap();
    m6::grant_library_access(&db, user, LibraryId(lib)).await.unwrap();
    let start = now_ms() + 3600000;
    programme(&db, ch, "Retried", start, start + 60000).await;
    let rule=app.call(Method::POST,"/api/v1/livetv/rules",Some(&child),Some(json!({"title":"Retried","channelId":ch,"keepMax":2,"autoDeleteWatched":true,"startPaddingSecs":30,"endPaddingSecs":60}))).await;
    assert_eq!(rule.status, StatusCode::OK, "{}", rule.body);
    let rule_id = rule.body["id"].as_i64().unwrap();
    let old = m9::list_recordings(&db, None)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.rule_id == Some(rule_id))
        .unwrap();
    let path = app.dir.path().join(format!("data/recordings/{}.ts", old.id));
    tokio::fs::write(&path, &source.bytes).await.unwrap();
    m9::update_recording_status(
        &db,
        RecordingId(old.id),
        "recording",
        Some(path.to_str().unwrap()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    services
        .livetv
        .publish_recording_capture(RecordingId(old.id), Some("test source ended"))
        .await
        .unwrap();
    let mut body = request(999, 999);
    body["retryOf"] = json!(old.id);
    let plan = app
        .call(Method::POST, "/api/v1/livetv/recordings/preview", Some(&admin), Some(body.clone()))
        .await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.body);
    assert_eq!(plan.body["ownerId"], user.0);
    assert_eq!(plan.body["ruleId"], rule_id);
    assert_eq!(plan.body["keepMax"], 2);
    assert_eq!(plan.body["autoDeleteWatched"], true);
    assert_eq!(plan.body["startTime"], start);
    assert_eq!(plan.body["startPaddingSecs"], 0);
    let scheduled =
        app.call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(body.clone())).await;
    assert_eq!(scheduled.status, StatusCode::OK, "{}", scheduled.body);
    let new_id = RecordingId(scheduled.body["id"].as_i64().unwrap());
    assert_ne!(new_id.0, old.id);
    let row = m9::get_recording(&db, new_id).await.unwrap().unwrap();
    assert_eq!(row.owner_id, Some(user.0));
    assert_eq!(row.rule_id, Some(rule_id));
    assert!(path.exists());
    let peer = app.create_user(&admin, "retry-peer").await;
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/livetv/recordings/preview",
            Some(&peer),
            Some(body.clone())
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    m6::set_user_libraries(&db, user, &[]).await.unwrap();
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/livetv/recordings/preview",
            Some(&admin),
            Some(body.clone())
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    m6::grant_library_access(&db, user, LibraryId(lib)).await.unwrap();
    let stale: mbl_core::RecordingPlan = serde_json::from_value(plan.body.clone()).unwrap();
    app.call(Method::DELETE, &format!("/api/v1/livetv/rules/{rule_id}"), Some(&child), None).await;
    let rejected = m9::schedule_owned_recording(
        &db,
        &m9::NewRecording {
            retry_of: Some(RecordingId(old.id)),
            owner_id: Some(user),
            library_id: Some(LibraryId(lib)),
            rule_id: stale.rule_id,
            channel_id: stale.channel_id,
            program_id: stale.program_id,
            title: &stale.title,
            start_time: stale.start_time,
            end_time: stale.end_time,
            program_start_time: stale.program_start_time,
            program_end_time: stale.program_end_time,
        },
        false,
    )
    .await;
    assert!(matches!(rejected, Err(mbl_core::Error::Conflict(_))));
    let manual =
        app.call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(body.clone())).await;
    assert_eq!(manual.status, StatusCode::OK, "{}", manual.body);
    let row = m9::get_recording(&db, RecordingId(manual.body["id"].as_i64().unwrap()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.owner_id, Some(user.0));
    assert!(row.rule_id.is_none());
    assert!(path.exists());
    sqlx::query("UPDATE epg_programs SET rating='R'").execute(&db.write).await.unwrap();
    assert_eq!(
        app.call(Method::POST, "/api/v1/livetv/recordings/preview", Some(&admin), Some(body))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn preview_reports_conflicts_and_reuses_existing_reservations() {
    let (app, db, _services, admin, ch) = fixture("http://127.0.0.1:1/unused", 512, None).await;
    let start = now_ms() + 3600000;
    let first = programme(&db, ch, "First", start, start + 60000).await;
    let second = programme(&db, ch, "Second", start + 1000, start + 61000).await;
    let scheduled = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(request(ch, first)))
        .await;
    assert_eq!(scheduled.status, StatusCode::OK);
    let duplicate = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings/preview",
            Some(&admin),
            Some(request(ch, first)),
        )
        .await;
    assert_eq!(duplicate.body["existingRecordingId"], scheduled.body["id"]);
    assert!(duplicate.body["conflict"].is_null());
    let conflict = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings/preview",
            Some(&admin),
            Some(request(ch, second)),
        )
        .await;
    assert_eq!(conflict.status, StatusCode::OK);
    assert!(conflict.body["conflict"].is_string());
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(request(ch, second))
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(count(&db, "recordings").await, 1);
}

async fn finished(db: &Db, id: RecordingId) -> m9::RecordingRow {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let row = m9::get_recording(db, id).await.unwrap().unwrap();
            if matches!(row.status.as_str(), "failed" | "partial" | "completed") {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}
fn launch(
    app: &TestApp,
    db: &Db,
    services: &mbl_service::Services,
    ffmpeg: PathBuf,
) -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let stop = CancellationToken::new();
    let task = tokio::spawn(mbl_server::recorder::run(
        db.clone(),
        services.livetv.clone(),
        services.playback.clone(),
        ffmpeg,
        app.dir.path().join("data/recordings"),
        stop.clone(),
    ));
    (stop, task)
}
#[cfg(unix)]
#[tokio::test]
async fn insufficient_space_prevents_start_without_deleting_any_saved_files() {
    use std::os::unix::fs::PermissionsExt;
    let (app, db, services, admin, ch) = fixture("http://127.0.0.1:1/unused", 1 << 40, None).await;
    let now = now_ms();
    let program = programme(&db, ch, "No space", now - 100, now + 60000).await;
    let plan = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings/preview",
            Some(&admin),
            Some(request(ch, program)),
        )
        .await;
    assert_eq!(plan.status, StatusCode::OK);
    assert_eq!(plan.body["storage"]["canStart"], false);
    let rec = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(request(ch, program)))
        .await;
    assert_eq!(rec.status, StatusCode::OK);
    let id = RecordingId(rec.body["id"].as_i64().unwrap());
    let flag = app.dir.path().join("spawned");
    let script = app.dir.path().join("unused.sh");
    std::fs::write(&script, format!("#!/bin/sh\ntouch '{}'\nexit 1\n", flag.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let kept = app.dir.path().join("data/recordings/existing.ts");
    tokio::fs::write(&kept, b"saved archive").await.unwrap();
    let (stop, task) = launch(&app, &db, &services, script);
    let row = finished(&db, id).await;
    stop.cancel();
    task.await.unwrap();
    assert_eq!(row.status, "failed");
    assert!(row.error_message.unwrap().contains("free space"));
    assert!(!flag.exists());
    assert!(kept.exists());
    assert!(!kept.with_file_name(format!("{}.ts", id.0)).exists());
}
#[tokio::test]
async fn per_recording_size_cap_preserves_a_labelled_playable_partial() {
    let source = common::live_source_with_duration(300).await;
    let (app, db, services, admin, ch) = fixture(&source.url, 512, Some(1)).await;
    let settings = mbl_mcp::execute_tool(
        "get_settings",
        &json!({}),
        &caller(services.auth.authenticate(&admin).await.unwrap().actor),
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(settings.structured_content.unwrap()["dvr"]["maxRecordingBytes"], 1024 * 1024);
    let now = now_ms();
    let program = programme(&db, ch, "Capped", now - 100, now + 300000).await;
    let rec = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(request(ch, program)))
        .await;
    assert_eq!(rec.status, StatusCode::OK, "{}", rec.body);
    let id = RecordingId(rec.body["id"].as_i64().unwrap());
    let (stop, task) = launch(&app, &db, &services, services.stream.ffmpeg_bin().to_path_buf());
    let row = finished(&db, id).await;
    stop.cancel();
    task.await.unwrap();
    assert_eq!(row.status, "partial", "{:?}", row.error_message);
    assert!(row.item_id.is_some());
    assert!(row.duration_ms.unwrap() < 300000);
    assert!(row.error_message.unwrap().contains("size limit"));
    assert!(
        row.size_bytes.unwrap() <= 1024 * 1024 + 128 * 1024,
        "packet overshoot must stay small"
    );
    let item = row.item_id.unwrap();
    let play = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":item,"profile":"chrome","waitForReady":true})),
        )
        .await;
    assert_eq!(play.status, StatusCode::OK, "{}", play.body);
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", play.body["sessionId"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn policy_failure_during_capture_reaps_the_child_without_publishing_a_partial() {
    use std::os::unix::fs::PermissionsExt;
    let source = common::live_source().await;
    let (app, db, services, admin, ch) = fixture(&source.url, 512, None).await;
    app.call(Method::POST, "/api/v1/livetv/rules", Some(&admin), Some(json!({"title":"Seed"})))
        .await;
    let lib: i64 = sqlx::query_scalar("SELECT library_id FROM dvr_libraries")
        .fetch_one(&db.read)
        .await
        .unwrap();
    let child = app.create_user(&admin, "capture-policy").await;
    let user = services.auth.authenticate(&child).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling='PG' WHERE id=?")
        .bind(user.0)
        .execute(&db.write)
        .await
        .unwrap();
    m6::grant_library_access(&db, user, LibraryId(lib)).await.unwrap();
    let now = now_ms();
    let p = programme(&db, ch, "PG programme", now - 100, now + 60000).await;
    sqlx::query("UPDATE epg_programs SET rating='PG' WHERE id=?")
        .bind(p)
        .execute(&db.write)
        .await
        .unwrap();
    let rec = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&child), Some(request(ch, p)))
        .await;
    assert_eq!(rec.status, StatusCode::OK, "{}", rec.body);
    let input = app.dir.path().join("input.ts");
    tokio::fs::write(&input, &source.bytes).await.unwrap();
    let ready = app.dir.path().join("capture-pid");
    let script = app.dir.path().join("capture.sh");
    std::fs::write(&script,format!("#!/bin/sh\nfor last; do true; done\ncp '{}' \"$last\"\necho $$ > '{}'\nexec sleep 30\n",input.display(),ready.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (stop, task) = launch(&app, &db, &services, script);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !ready.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid = tokio::fs::read_to_string(&ready).await.unwrap();
    let pid = pid.trim().parse::<u32>().unwrap();
    sqlx::query("UPDATE users SET content_rating_ceiling='G' WHERE id=?")
        .bind(user.0)
        .execute(&db.write)
        .await
        .unwrap();
    let row = finished(&db, RecordingId(rec.body["id"].as_i64().unwrap())).await;
    stop.cancel();
    task.await.unwrap();
    assert_eq!(row.status, "failed");
    assert!(row.item_id.is_none());
    #[cfg(target_os = "linux")]
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
}
