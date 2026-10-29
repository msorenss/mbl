#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_core::{RecordingId, time::now_ms};
use mbl_db::{Db, repo::m9};
use serde_json::json;
use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

async fn failed() -> (TestApp, Db, mbl_service::Services, String, RecordingId, PathBuf, Vec<u8>) {
    let source = common::live_source().await;
    let bytes = source.bytes.to_vec();
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let imported=app.call(Method::POST,"/api/v1/livetv/tuners/m3u",Some(&admin),Some(json!({"name":"Recovery tests","content":format!("#EXTM3U\n#EXTINF:-1,Recovery\n{}\n",source.url)}))).await;
    assert_eq!(imported.status, StatusCode::OK);
    let channel: i64 =
        sqlx::query_scalar("SELECT id FROM channels LIMIT 1").fetch_one(&db.read).await.unwrap();
    let now = now_ms();
    let rec=app.call(Method::POST,"/api/v1/livetv/recordings",Some(&admin),Some(json!({"channelId":channel,"title":"Interrupted broadcast","startTime":now+1000,"endTime":now+120000}))).await;
    assert_eq!(rec.status, StatusCode::OK, "{}", rec.body);
    let id = RecordingId(rec.body["id"].as_i64().unwrap());
    let path = app.recordings_dir().join(format!("{}.ts", id.0));
    tokio::fs::write(&path, &bytes).await.unwrap();
    m9::update_recording_status(
        &db,
        id,
        "failed",
        Some(path.to_str().unwrap()),
        Some(bytes.len() as u64),
        None,
        Some("Recording interrupted by server restart"),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE recordings SET content_rating='G' WHERE id=?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    // The source goes away: recovery must read saved media, not reconnect to the channel.
    drop(source);
    (app, db, services, admin, id, path, bytes)
}

fn caller(actor: mbl_core::Actor) -> mbl_mcp::Caller {
    mbl_mcp::Caller {
        actor,
        scopes: mbl_auth::tokens::ScopeSet::full_for_user(true),
        client: "recovery-test".into(),
        token_id: None,
    }
}

#[tokio::test]
async fn recovery_preview_is_read_only_and_the_snapshot_is_playable_and_immutable() {
    let (app, db, services, admin, id, path, bytes) = failed().await;
    let archive = path.with_file_name(format!("{}.recovered.ts", id.0));
    let url = format!("/api/v1/livetv/recordings/{}/recovery", id.0);
    let plan = app.call(Method::GET, &url, Some(&admin), None).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.body);
    assert_eq!(plan.body["canRecover"], true);
    assert_eq!(plan.body["sizeBytes"], bytes.len());
    let actor = services.auth.authenticate(&admin).await.unwrap().actor;
    let unconfirmed = mbl_mcp::execute_tool(
        "recover_recording",
        &json!({"recording_id":id.0}),
        &caller(actor.clone()),
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(unconfirmed.is_error);
    let dry = mbl_mcp::execute_tool(
        "recover_recording",
        &json!({"recording_id":id.0,"dry_run":true}),
        &caller(actor),
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(dry.structured_content.unwrap()["plan"]["sizeBytes"], bytes.len());
    assert!(!archive.exists());
    assert!(!path.parent().unwrap().join(".recovery-staging").exists());
    assert!(m9::get_recording(&db, id).await.unwrap().unwrap().item_id.is_none());
    let channel = m9::get_recording(&db, id).await.unwrap().unwrap().channel_id.unwrap();
    assert_eq!(
        app.call(Method::DELETE, &format!("/api/v1/livetv/channels/{channel}"), Some(&admin), None)
            .await
            .status,
        StatusCode::OK
    );
    let recovered = app.call(Method::POST, &url, Some(&admin), None).await;
    assert_eq!(recovered.status, StatusCode::OK, "{}", recovered.body);
    assert_eq!(recovered.body["status"], "partial");
    assert_eq!(recovered.body["id"], id.0);
    assert_eq!(tokio::fs::read(&archive).await.unwrap(), bytes);
    let again = app.call(Method::POST, &url, Some(&admin), None).await;
    assert_eq!(again.body["itemId"], recovered.body["itemId"]);
    let items: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items").fetch_one(&db.read).await.unwrap();
    assert_eq!(items, 1);
    // An orphaned capture writer cannot change the published snapshot by appending to its raw path.
    tokio::fs::write(&path, b"changed by old writer").await.unwrap();
    let play = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":recovered.body["itemId"],"profile":"chrome","waitForReady":true})),
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
    let staging = path.parent().unwrap().join(".recovery-staging");
    tokio::fs::write(staging.join(format!(".recover-{}-ABC123.tmp", id.0)), b"interrupted scratch")
        .await
        .unwrap();
    let unknown = staging.join("keep-this.txt");
    tokio::fs::write(&unknown, b"not owned by cleanup").await.unwrap();
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
    assert!(!archive.exists());
    assert!(!staging.join(format!(".recover-{}-ABC123.tmp", id.0)).exists());
    assert!(unknown.exists());
}

#[tokio::test]
async fn simultaneous_recovery_requests_keep_one_snapshot_and_catalogue_item() {
    let (app, db, services, admin, id, path, _) = failed().await;
    let url = format!("/api/v1/livetv/recordings/{}/recovery", id.0);
    let actor = services.auth.authenticate(&admin).await.unwrap().actor;
    let rule:i64=sqlx::query_scalar("INSERT INTO recording_rules (user_id,title,keep_max,created_at) VALUES (?,'Keep recovery',2,?) RETURNING id").bind(actor.user_id.0).bind(now_ms()).fetch_one(&db.write).await.unwrap();
    sqlx::query("UPDATE recordings SET rule_id=? WHERE id=?")
        .bind(rule)
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    let args = json!({"recording_id":id.0,"confirm":true});
    let caller = caller(actor.clone());
    let (a, b) = tokio::join!(
        app.call(Method::POST, &url, Some(&admin), None),
        mbl_mcp::execute_tool("recover_recording", &args, &caller, &db, &services, None, None)
    );
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    let b = b.unwrap();
    assert!(!b.is_error, "{b:?}");
    assert_eq!(a.body["itemId"], b.structured_content.unwrap()["itemId"]);
    assert_eq!(a.body["ruleId"], rule);
    assert_eq!(m9::get_recording(&db, id).await.unwrap().unwrap().owner_id, Some(actor.user_id.0));
    assert!(path.with_file_name(format!("{}.recovered.ts", id.0)).exists());
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items").fetch_one(&db.read).await.unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn missing_empty_corrupt_and_low_space_captures_never_create_a_playable_item() {
    let (app, db, services, admin, id, path, bytes) = failed().await;
    let actor = services.auth.authenticate(&admin).await.unwrap().actor;
    let limited =
        services.livetv.clone().with_recording_storage(mbl_core::livetv::DvrStoragePolicy {
            min_free_bytes: u64::MAX,
            max_recording_bytes: None,
        });
    let preview = limited.preview_recording_recovery(&actor, id).await.unwrap();
    assert!(!preview.can_recover);
    assert!(limited.recover_recording(&actor, id).await.is_err());
    assert!(!path.with_file_name(format!("{}.recovered.ts", id.0)).exists());
    let url = format!("/api/v1/livetv/recordings/{}/recovery", id.0);
    tokio::fs::remove_file(&path).await.unwrap();
    let plan = app.call(Method::GET, &url, Some(&admin), None).await;
    assert_eq!(plan.body["canRecover"], false);
    tokio::fs::write(&path, b"").await.unwrap();
    assert_eq!(app.call(Method::GET, &url, Some(&admin), None).await.body["canRecover"], false);
    tokio::fs::write(&path, b"not media").await.unwrap();
    assert_eq!(
        app.call(Method::POST, &url, Some(&admin), None).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(m9::get_recording(&db, id).await.unwrap().unwrap().item_id.is_none());
    assert_eq!(tokio::fs::read(&path).await.unwrap(), b"not media");
    assert_ne!(bytes, b"not media");
}

#[tokio::test]
async fn deleted_capture_files_are_retried_before_the_tombstone_is_pruned() {
    let (app, db, services, _admin, id, path, bytes) = failed().await;
    m9::delete_recording(&db, id).await.unwrap();
    sqlx::query("UPDATE recordings SET program_end_time=?,end_time=? WHERE id=?")
        .bind(now_ms() - 172800000)
        .bind(now_ms() - 172800000)
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    tokio::fs::remove_file(&path).await.unwrap();
    // A non-file obstructs cleanup. No recursive removal or loss of the durable intent.
    tokio::fs::create_dir(&path).await.unwrap();
    services.livetv.reconcile_recordings().await;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recordings WHERE id=?")
        .bind(id.0)
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert!(path.is_dir());
    tokio::fs::remove_dir(&path).await.unwrap();
    tokio::fs::write(&path, &bytes).await.unwrap();
    let archive = path.with_file_name(format!("{}.recovered.ts", id.0));
    tokio::fs::write(&archive, &bytes).await.unwrap();
    let unrelated = app.recordings_dir().join("unrelated.ts");
    tokio::fs::write(&unrelated, b"keep").await.unwrap();
    services.livetv.reconcile_recordings().await;
    assert!(!path.exists());
    assert!(!archive.exists());
    assert!(unrelated.exists());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recordings WHERE id=?")
        .bind(id.0)
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn recovery_rechecks_the_owner_during_probe_and_refuses_shared_viewers() {
    use std::os::unix::fs::PermissionsExt;
    let (app, db, services, admin, id, path, _) = failed().await;
    let token = app.create_user(&admin, "recovery-owner").await;
    let owner = services.auth.authenticate(&token).await.unwrap().actor.user_id;
    let row = m9::get_recording(&db, id).await.unwrap().unwrap();
    sqlx::query("UPDATE recordings SET owner_id=? WHERE id=?")
        .bind(owner.0)
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling='G' WHERE id=?")
        .bind(owner.0)
        .execute(&db.write)
        .await
        .unwrap();
    mbl_db::repo::m6::grant_library_access(
        &db,
        owner,
        mbl_core::LibraryId(row.library_id.unwrap()),
    )
    .await
    .unwrap();
    let peer = app.create_user(&admin, "recovery-viewer").await;
    let url = format!("/api/v1/livetv/recordings/{}/recovery", id.0);
    assert_eq!(app.call(Method::GET, &url, Some(&peer), None).await.status, StatusCode::NOT_FOUND);
    let started = app.dir.path().join("probe.started");
    let release = app.dir.path().join("probe.release");
    let script = app.dir.path().join("ffprobe.sh");
    tokio::fs::write(&script,format!("#!/bin/sh\ntouch '{}'\nwhile [ ! -f '{}' ]; do sleep 0.02; done\nexec ffprobe \"$@\"\n",started.display(),release.display())).await.unwrap();
    tokio::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).await.unwrap();
    let mut tools = mbl_probe::FfTools::locate("", "").await.unwrap();
    tools.ffprobe = script;
    let service =
        services.livetv.clone().with_recordings(path.parent().unwrap().to_path_buf(), Some(tools));
    let actor = services.auth.authenticate(&admin).await.unwrap().actor;
    let pending = tokio::spawn(async move { service.recover_recording(&actor, id).await });
    tokio::time::timeout(Duration::from_secs(10), async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    mbl_db::repo::m6::set_user_libraries(&db, owner, &[]).await.unwrap();
    tokio::fs::write(release, b"go").await.unwrap();
    assert!(pending.await.unwrap().is_err());
    assert!(m9::get_recording(&db, id).await.unwrap().unwrap().item_id.is_none());
    mbl_db::repo::m6::grant_library_access(
        &db,
        owner,
        mbl_core::LibraryId(row.library_id.unwrap()),
    )
    .await
    .unwrap();
    let restored = app.call(Method::POST, &url, Some(&admin), None).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.body);
}
