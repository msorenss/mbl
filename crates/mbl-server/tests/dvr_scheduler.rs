#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_core::{Actor, ChannelId, LibraryId, RecordingId, UserId, time::now_ms};
use mbl_db::{
    Db,
    repo::{m6, m9},
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

async fn fixture() -> (TestApp, Db, mbl_service::Services, String, i64, i64) {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let tuner=app.call(Method::POST,"/api/v1/livetv/tuners/m3u",Some(&admin),Some(json!({"name":"DVR tests","content":"#EXTM3U\n#EXTINF:-1 tvg-id=\"one\",One\nhttp://127.0.0.1:1/one.ts\n#EXTINF:-1 tvg-id=\"two\",Two\nhttp://127.0.0.1:1/two.ts\n"}))).await;
    assert_eq!(tuner.status, StatusCode::OK, "{}", tuner.body);
    sqlx::query("UPDATE tuners SET tuner_count=16").execute(&db.write).await.unwrap();
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM channels ORDER BY id")
        .fetch_all(&db.read)
        .await
        .unwrap();
    (app, db, services, admin, ids[0], ids[1])
}
#[allow(clippy::too_many_arguments)]
async fn programme(
    db: &Db,
    channel: i64,
    title: &str,
    start: i64,
    rating: Option<&str>,
    season: Option<u32>,
    episode: Option<u32>,
    new: bool,
) -> i64 {
    m9::batch_upsert_epg_programs(
        db,
        &[m9::EpgProgramInsert {
            channel_id: Some(ChannelId(channel)),
            epg_channel_id: format!("c{channel}"),
            title: title.into(),
            description: Some("Guide description".into()),
            season_number: season,
            episode_number: episode,
            start_time: start,
            end_time: start + 30_000,
            icon_url: None,
            rating: rating.map(str::to_owned),
            genres: vec![],
            is_new: new,
        }],
    )
    .await
    .unwrap();
    sqlx::query_scalar("SELECT id FROM epg_programs WHERE channel_id=? AND start_time=?")
        .bind(channel)
        .bind(start)
        .fetch_one(&db.read)
        .await
        .unwrap()
}
fn request(channel: i64, program: i64) -> Value {
    json!({"channelId":channel,"programId":program,"title":"Spoofed title","startTime":1,"endTime":2,"startPaddingSecs":120,"endPaddingSecs":300})
}
async fn library(db: &Db) -> i64 {
    sqlx::query_scalar("SELECT library_id FROM dvr_libraries LIMIT 1")
        .fetch_one(&db.read)
        .await
        .unwrap()
}
async fn make_profile(
    app: &TestApp,
    db: &Db,
    admin: &str,
    name: &str,
    rating: Option<&str>,
    lib: i64,
) -> (String, UserId) {
    let token = app.create_user(admin, name).await;
    let id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username=?")
        .bind(name)
        .fetch_one(&db.read)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling=? WHERE id=?")
        .bind(rating)
        .bind(id)
        .execute(&db.write)
        .await
        .unwrap();
    m6::grant_library_access(db, UserId(id), LibraryId(lib)).await.unwrap();
    (token, UserId(id))
}
async fn publish(
    app: &TestApp,
    db: &Db,
    services: &mbl_service::Services,
    id: i64,
    bytes: &[u8],
) -> (i64, std::path::PathBuf) {
    let path = app.dir.path().join(format!("data/recordings/{id}.ts"));
    tokio::fs::create_dir_all(path.parent().unwrap()).await.unwrap();
    tokio::fs::write(&path, bytes).await.unwrap();
    m9::update_recording_status(
        db,
        RecordingId(id),
        "completed",
        Some(path.to_str().unwrap()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let item = services.livetv.publish_recording(RecordingId(id)).await.unwrap();
    (item.0, path)
}

#[tokio::test]
async fn profiles_use_real_guide_metadata_and_own_pending_recordings() {
    let (app, db, services, admin, channel, other) = fixture().await;
    let starts = now_ms() + 3_600_000;
    let good = programme(&db, channel, "Safe", starts, Some("G"), Some(1), Some(1), true).await;
    let bad =
        programme(&db, channel, "Adult", starts + 60_000, Some("R"), Some(1), Some(2), true).await;
    let unknown =
        programme(&db, channel, "Unknown", starts + 120_000, None, None, None, false).await;
    let seeded = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(request(channel, good)))
        .await;
    assert_eq!(seeded.status, StatusCode::OK, "{}", seeded.body);
    let lib = library(&db).await;
    let (alice, user) = make_profile(&app, &db, &admin, "dvr-alice", Some("G"), lib).await;
    let (bob, _) = make_profile(&app, &db, &admin, "dvr-bob", Some("G"), lib).await;
    let scheduled = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&alice), Some(request(channel, good)))
        .await;
    assert_eq!(scheduled.status, StatusCode::OK, "{}", scheduled.body);
    assert_eq!(scheduled.body["title"], "Safe");
    assert_eq!(scheduled.body["startTime"], starts);
    assert_eq!(scheduled.body["endTime"], starts + 30_000);
    assert_eq!(scheduled.body["canDelete"], true);
    let id = scheduled.body["id"].as_i64().unwrap();
    for body in [request(channel, bad), request(channel, unknown), request(other, good)] {
        assert_eq!(
            app.call(Method::POST, "/api/v1/livetv/recordings", Some(&alice), Some(body))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(app.call(Method::POST,"/api/v1/livetv/recordings",Some(&alice),Some(json!({"channelId":channel,"title":"Manual","startTime":starts,"endTime":starts+30_000}))).await.status,StatusCode::FORBIDDEN);
    assert_eq!(
        app.call(Method::GET, "/api/v1/livetv/recordings", Some(&alice), None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        app.call(Method::GET, "/api/v1/livetv/recordings", Some(&bob), None)
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.call(Method::DELETE, &format!("/api/v1/livetv/recordings/{id}"), Some(&bob), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    m6::set_user_libraries(&db, user, &[]).await.unwrap();
    let row = m9::get_recording(&db, RecordingId(id)).await.unwrap().unwrap();
    assert!(services.livetv.authorize_recording_job(&row).await.is_err());
    assert!(
        app.call(Method::GET, "/api/v1/livetv/recordings", Some(&alice), None)
            .await
            .body
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (transition, transition_user) =
        make_profile(&app, &db, &admin, "changed-age-policy", None, lib).await;
    let manual=app.call(Method::POST,"/api/v1/livetv/recordings",Some(&transition),Some(json!({"channelId":channel,"title":"Manual span","startTime":starts,"endTime":starts+60000,"startPaddingSecs":120,"endPaddingSecs":300}))).await;
    assert_eq!(manual.status, StatusCode::OK, "{}", manual.body);
    let manual = m9::get_recording(&db, RecordingId(manual.body["id"].as_i64().unwrap()))
        .await
        .unwrap()
        .unwrap();
    // An unrestricted manual span may cross programme boundaries, even when a
    // guide entry happens to start at the same instant.
    services.livetv.authorize_recording_job(&manual).await.unwrap();
    sqlx::query("UPDATE users SET content_rating_ceiling='PG' WHERE id=?")
        .bind(transition_user.0)
        .execute(&db.write)
        .await
        .unwrap();
    assert!(services.livetv.authorize_recording_job(&manual).await.is_err());
    // A rating change must not let an unrestricted owner publish newly adult
    // content with the old G snapshot into a shared children's library view.
    sqlx::query("UPDATE epg_programs SET rating='R' WHERE id=?")
        .bind(good)
        .execute(&db.write)
        .await
        .unwrap();
    let admin_row = m9::get_recording(&db, RecordingId(seeded.body["id"].as_i64().unwrap()))
        .await
        .unwrap()
        .unwrap();
    assert!(services.livetv.authorize_recording_job(&admin_row).await.is_err());
}

#[tokio::test]
async fn recurring_rules_page_reimported_guide_and_respect_cancellation_and_explicit_retry() {
    let (app, db, services, admin, channel, _) = fixture().await;
    let rule=app.call(Method::POST,"/api/v1/livetv/rules",Some(&admin),Some(json!({"title":"Many episodes","channelId":channel,"seriesOnly":true,"seasonNumber":2,"newOnly":true,"startPaddingSecs":0,"endPaddingSecs":0}))).await;
    assert_eq!(rule.status, StatusCode::OK, "{}", rule.body);
    let starts = now_ms() + 3_600_000;
    let mut first = 0;
    for n in 0..205 {
        let id = programme(
            &db,
            channel,
            "Many episodes",
            starts + n * 60_000,
            Some("G"),
            Some(2),
            Some(n as u32 + 1),
            true,
        )
        .await;
        if n == 0 {
            first = id;
        }
    }
    programme(&db, channel, "Many episodes", starts, Some("G"), Some(2), Some(1), true).await;
    programme(
        &db,
        channel,
        "Many episodes",
        starts + 300 * 60_000,
        Some("G"),
        Some(1),
        Some(1),
        true,
    )
    .await;
    programme(&db, channel, "Many episodes", starts + 301 * 60_000, Some("G"), Some(2), None, true)
        .await;
    programme(
        &db,
        channel,
        "Many episodes",
        starts + 302 * 60_000,
        Some("G"),
        Some(2),
        Some(2),
        false,
    )
    .await;
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    let rows = m9::list_recordings(&db, None).await.unwrap();
    assert_eq!(rows.len(), 205);
    assert!(rows.iter().all(|r| r.status == "scheduled"));
    let original = rows.iter().find(|r| r.program_id == Some(first)).unwrap().id;
    let deleted = app
        .call(Method::DELETE, &format!("/api/v1/livetv/recordings/{original}"), Some(&admin), None)
        .await;
    assert_eq!(deleted.status, StatusCode::OK);
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    assert_eq!(m9::list_recordings(&db, None).await.unwrap().len(), 204);
    let retry = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(request(channel, first)),
        )
        .await;
    assert_eq!(retry.status, StatusCode::OK, "{}", retry.body);
    assert!(retry.body["id"].as_i64().unwrap() > original);
    assert_eq!(m9::list_recordings(&db, None).await.unwrap().len(), 205);
}

#[tokio::test]
async fn conflicts_are_visible_and_duplicate_requests_do_not_use_an_extra_tuner() {
    let (app, db, services, admin, channel, other) = fixture().await;
    sqlx::query("UPDATE tuners SET tuner_count=1").execute(&db.write).await.unwrap();
    let starts = now_ms() + 3_600_000;
    let first = programme(&db, channel, "First", starts, Some("G"), None, None, false).await;
    let second = programme(&db, other, "Conflict", starts, Some("G"), None, None, false).await;
    let one = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(request(channel, first)),
        )
        .await;
    let duplicate = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(request(channel, first)),
        )
        .await;
    assert_eq!(one.status, StatusCode::OK, "{}", one.body);
    assert_eq!(duplicate.status, StatusCode::OK, "{}", duplicate.body);
    assert_eq!(one.body["id"], duplicate.body["id"]);
    let rule = app
        .call(
            Method::POST,
            "/api/v1/livetv/rules",
            Some(&admin),
            Some(json!({"title":"Conflict","channelId":other})),
        )
        .await;
    assert_eq!(rule.status, StatusCode::OK, "{}", rule.body);
    let rows = m9::list_recordings(&db, Some("failed")).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].error_message.as_ref().unwrap().contains("Tuner conflict"));
    services
        .livetv
        .cancel_recording(&Actor::admin(UserId(1)), RecordingId(one.body["id"].as_i64().unwrap()))
        .await
        .unwrap();
    let retry = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(request(other, second)))
        .await;
    assert_eq!(retry.status, StatusCode::OK, "{}", retry.body);
    assert_eq!(retry.body["status"], "scheduled");
}

#[tokio::test]
async fn retention_keeps_latest_files_and_preserves_active_playback() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel, _) = fixture().await;
    let starts = now_ms() + 3_600_000;
    for n in 0..3 {
        programme(
            &db,
            channel,
            "Retained",
            starts + n * 60_000,
            Some("G"),
            Some(1),
            Some(n as u32),
            true,
        )
        .await;
    }
    let rule = app
        .call(
            Method::POST,
            "/api/v1/livetv/rules",
            Some(&admin),
            Some(json!({"title":"Retained","channelId":channel,"keepMax":2})),
        )
        .await;
    assert_eq!(rule.status, StatusCode::OK, "{}", rule.body);
    let mut rows = m9::list_recordings(&db, None).await.unwrap();
    rows.sort_by_key(|r| r.start_time);
    let mut media = Vec::new();
    for row in rows {
        media.push(publish(&app, &db, &services, row.id, &source.bytes).await);
    }
    sqlx::query(
        "UPDATE recordings SET status='partial',error_message='Incomplete source' WHERE item_id=?",
    )
    .bind(media[0].0)
    .execute(&db.write)
    .await
    .unwrap();
    let active = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":media[0].0,"profile":"chrome","waitForReady":true})),
        )
        .await;
    assert_eq!(active.status, StatusCode::OK, "{}", active.body);
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    assert!(media[0].1.exists());
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", active.body["sessionId"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    assert!(!media[0].1.exists());
    assert!(media[1].1.exists());
    assert!(media[2].1.exists());
    assert_eq!(m9::list_recordings(&db, Some("completed")).await.unwrap().len(), 2);
}

#[tokio::test]
async fn runtime_matches_later_guide_imports_and_rechecks_revoked_profiles() {
    let (app, db, services, admin, channel, _) = fixture().await;
    let rule = app
        .call(
            Method::POST,
            "/api/v1/livetv/rules",
            Some(&admin),
            Some(json!({"title":"Later","channelId":channel})),
        )
        .await;
    assert_eq!(rule.status, StatusCode::OK);
    let lib = library(&db).await;
    let (alice, user) = make_profile(&app, &db, &admin, "revoked-dvr", Some("G"), lib).await;
    let starts = now_ms() + 3_600_000;
    let program = programme(&db, channel, "Revoked", starts, Some("G"), None, None, false).await;
    let scheduled = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&alice),
            Some(request(channel, program)),
        )
        .await;
    assert_eq!(scheduled.status, StatusCode::OK, "{}", scheduled.body);
    let id = scheduled.body["id"].as_i64().unwrap();
    m6::set_user_libraries(&db, user, &[]).await.unwrap();
    sqlx::query("UPDATE recordings SET start_time=?,end_time=? WHERE id=?")
        .bind(now_ms() - 100)
        .bind(now_ms() + 60_000)
        .bind(id)
        .execute(&db.write)
        .await
        .unwrap();
    programme(&db, channel, "Later", starts + 60_000, Some("G"), None, None, false).await;
    let stop = CancellationToken::new();
    let task = tokio::spawn(mbl_server::recorder::run(
        db.clone(),
        services.livetv.clone(),
        services.playback.clone(),
        services.stream.ffmpeg_bin().to_path_buf(),
        app.dir.path().join("data/recordings"),
        stop.clone(),
    ));
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let rows = m9::list_recordings(&db, None).await.unwrap();
            if rows.iter().any(|r| r.title == "Later")
                && rows.iter().any(|r| r.id == id && r.status == "failed")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert!(!app.dir.path().join(format!("data/recordings/{id}.ts")).exists());
    stop.cancel();
    task.await.unwrap();
}

#[tokio::test]
async fn watched_retention_uses_only_the_rule_owner_and_shared_viewers_cannot_delete() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel, _) = fixture().await;
    let starts = now_ms() + 3_600_000;
    let p = programme(&db, channel, "Seed", starts, Some("G"), None, None, false).await;
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(request(channel, p))
        )
        .await
        .status,
        StatusCode::OK
    );
    let lib = library(&db).await;
    let (alice, _) = make_profile(&app, &db, &admin, "watched-owner", None, lib).await;
    let (bob, _) = make_profile(&app, &db, &admin, "watched-viewer", None, lib).await;
    programme(&db, channel, "Shared", starts + 60_000, Some("G"), None, None, false).await;
    let rule = app
        .call(
            Method::POST,
            "/api/v1/livetv/rules",
            Some(&alice),
            Some(json!({"title":"Shared","channelId":channel,"autoDeleteWatched":true})),
        )
        .await;

    assert_eq!(rule.status, StatusCode::OK, "{}", rule.body);
    let row = m9::list_recordings(&db, None)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.title == "Shared")
        .unwrap();
    let (item, path) = publish(&app, &db, &services, row.id, &source.bytes).await;
    sqlx::query(
        "UPDATE recordings SET status='partial',error_message='Incomplete source' WHERE id=?",
    )
    .bind(row.id)
    .execute(&db.write)
    .await
    .unwrap();
    let get = app
        .call(Method::GET, &format!("/api/v1/livetv/recordings/{}", row.id), Some(&bob), None)
        .await;
    assert_eq!(get.status, StatusCode::OK);
    assert_eq!(get.body["canDelete"], false);
    assert_eq!(
        app.call(
            Method::DELETE,
            &format!("/api/v1/livetv/recordings/{}", row.id),
            Some(&bob),
            None
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE users SET content_rating_ceiling='PG' WHERE username='watched-viewer'")
        .execute(&db.write)
        .await
        .unwrap();
    for (rating, allowed) in [
        (Some("TV-PG"), true),
        (Some("custom-unclassified"), false),
        (None, true),
        (Some("G"), true),
    ] {
        sqlx::query("UPDATE media_items SET content_rating=? WHERE id=?")
            .bind(rating)
            .bind(item)
            .execute(&db.write)
            .await
            .unwrap();
        let listed = app.call(Method::GET, "/api/v1/livetv/recordings", Some(&bob), None).await;
        let visible = listed.body.as_array().unwrap().iter().any(|r| r["id"] == row.id);
        assert_eq!(
            visible, allowed,
            "rating {rating:?} must use the same policy for browse and playback"
        );
    }
    assert_eq!(
        app.call(Method::POST, &format!("/api/v1/items/{item}/watched"), Some(&bob), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    assert!(path.exists());
    assert_eq!(
        app.call(Method::POST, &format!("/api/v1/items/{item}/watched"), Some(&alice), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let active = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&bob),
            Some(json!({"itemId":item,"profile":"chrome","waitForReady":true})),
        )
        .await;
    assert_eq!(active.status, StatusCode::OK, "{}", active.body);
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    assert!(path.exists());
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", active.body["sessionId"].as_str().unwrap()),
        Some(&bob),
        None,
    )
    .await;
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    assert!(!path.exists());
    services.playback.maintain_dvr(&services.livetv).await.unwrap();
    assert!(m9::list_recordings(&db, None).await.unwrap().iter().all(|r| r.title != "Shared"));
}

#[tokio::test]
async fn managed_library_is_stable_and_completed_files_survive_tuner_removal() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel, _) = fixture().await;
    let starts = now_ms() + 3_600_000;
    let p = programme(&db, channel, "Archive", starts, Some("G"), None, None, false).await;
    let r = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(request(channel, p)))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let id = r.body["id"].as_i64().unwrap();
    let (item, path) = publish(&app, &db, &services, id, &source.bytes).await;
    let p =
        programme(&db, channel, "Pending", starts + 600_000, Some("G"), None, None, false).await;
    let pending = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&admin), Some(request(channel, p)))
        .await;
    assert_eq!(pending.status, StatusCode::OK, "{}", pending.body);
    let lib = library(&db).await;
    assert_eq!(
        app.call(
            Method::PATCH,
            &format!("/api/v1/libraries/{lib}"),
            Some(&admin),
            Some(json!({"paths":[app.dir.path().to_str().unwrap()]}))
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.call(Method::DELETE, &format!("/api/v1/libraries/{lib}"), Some(&admin), None)
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.call(
            Method::PATCH,
            &format!("/api/v1/libraries/{lib}"),
            Some(&admin),
            Some(json!({"name":"Saved TV"}))
        )
        .await
        .status,
        StatusCode::OK
    );
    let tuner: i64 = sqlx::query_scalar("SELECT tuner_id FROM channels WHERE id=?")
        .bind(channel)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let deleted = app
        .call(Method::DELETE, &format!("/api/v1/livetv/tuners/{tuner}"), Some(&admin), None)
        .await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.body);
    let archive =
        app.call(Method::GET, &format!("/api/v1/livetv/recordings/{id}"), Some(&admin), None).await;
    assert_eq!(archive.status, StatusCode::OK, "{}", archive.body);
    assert!(archive.body["channelId"].is_null());
    assert_eq!(archive.body["channelName"], "One");
    let pending = m9::get_recording(&db, RecordingId(pending.body["id"].as_i64().unwrap()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.status, "failed");
    assert!(path.exists());
    let playback = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":item,"waitForReady":true,"profile":"chrome"})),
        )
        .await;
    assert_eq!(playback.status, StatusCode::OK, "{}", playback.body);
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", playback.body["sessionId"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
}

#[tokio::test]
async fn rule_access_and_deletion_preserve_saved_media_and_cancel_only_pending_jobs() {
    let source = common::live_source().await;
    let (app, db, services, admin, channel, _) = fixture().await;
    let starts = now_ms() + 3_600_000;
    for n in 0..2 {
        programme(&db, channel, "Delete rule", starts + n * 60_000, Some("G"), None, None, false)
            .await;
    }
    let rule = app
        .call(
            Method::POST,
            "/api/v1/livetv/rules",
            Some(&admin),
            Some(json!({"title":"Delete rule","channelId":channel})),
        )
        .await;
    assert_eq!(rule.status, StatusCode::OK, "{}", rule.body);
    let rid = rule.body["id"].as_i64().unwrap();
    let stranger = app.create_user(&admin, "rule-stranger").await;
    for method in [Method::GET, Method::DELETE] {
        assert_eq!(
            app.call(method, &format!("/api/v1/livetv/rules/{rid}"), Some(&stranger), None)
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    let rows = m9::list_recordings(&db, None).await.unwrap();
    let (item, path) = publish(&app, &db, &services, rows[0].id, &source.bytes).await;
    assert_eq!(
        app.call(Method::DELETE, &format!("/api/v1/livetv/rules/{rid}"), Some(&admin), None)
            .await
            .status,
        StatusCode::OK
    );
    assert!(path.exists());
    let rows = m9::list_recordings(&db, None).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].item_id, Some(item));
    let recreated = app
        .call(
            Method::POST,
            "/api/v1/livetv/rules",
            Some(&admin),
            Some(json!({"title":"Delete rule","channelId":channel})),
        )
        .await;
    assert_eq!(recreated.status, StatusCode::OK, "{}", recreated.body);
    assert_eq!(m9::list_recordings(&db, None).await.unwrap().len(), 2);
}

#[tokio::test]
async fn removed_owners_cannot_turn_pending_jobs_into_anonymous_system_recordings() {
    let (app, db, _services, admin, channel, _) = fixture().await;
    let starts = now_ms() + 3_600_000;
    let p = programme(&db, channel, "Owner", starts, Some("G"), None, None, false).await;
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(request(channel, p))
        )
        .await
        .status,
        StatusCode::OK
    );
    let (user, id) =
        make_profile(&app, &db, &admin, "deleted-owner", None, library(&db).await).await;
    let scheduled = app
        .call(Method::POST, "/api/v1/livetv/recordings", Some(&user), Some(request(channel, p)))
        .await;
    assert_eq!(scheduled.status, StatusCode::OK, "{}", scheduled.body);
    sqlx::query("DELETE FROM users WHERE id=?").bind(id.0).execute(&db.write).await.unwrap();
    let row = m9::get_recording(&db, RecordingId(scheduled.body["id"].as_i64().unwrap()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "failed");
    assert!(row.owner_id.is_none());
    assert!(row.error_message.unwrap().contains("owner was removed"));
}

#[tokio::test]
async fn schema_upgrade_preserves_archives_and_deduplicates_guide_occurrences() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await.unwrap();
    for source in [
        include_str!("../../mbl-db/migrations/0001_init.sql"),
        include_str!("../../mbl-db/migrations/0002_mcp.sql"),
        include_str!("../../mbl-db/migrations/0003_hw_capabilities.sql"),
        include_str!("../../mbl-db/migrations/0004_m5_media_intelligence.sql"),
        include_str!("../../mbl-db/migrations/0005_m6_users_remote.sql"),
        include_str!("../../mbl-db/migrations/0006_m7_media_breadth.sql"),
        include_str!("../../mbl-db/migrations/0007_m8_distributed_nodes.sql"),
        include_str!("../../mbl-db/migrations/0008_m9_livetv_channels_plugins.sql"),
        include_str!("../../mbl-db/migrations/0009_matchmaker_membership.sql"),
        include_str!("../../mbl-db/migrations/0010_dvr_playback.sql"),
    ] {
        sqlx::raw_sql(source).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO users(id,username,display_name,password_hash,is_admin,created_at) VALUES (1,'admin','Admin','x',1,0),(2,'owner','Owner','x',0,0); INSERT INTO libraries(id,name,kind,created_at) VALUES (1,'Archive','homevideo',0); INSERT INTO media_items(id,library_id,kind,title,sort_title,added_at,updated_at) VALUES (7,1,'movie','Saved','saved',0,0); INSERT INTO channels(id,name,channel_number,stream_url,created_at,updated_at) VALUES (3,'Old channel','3','http://example.test/a',0,0); INSERT INTO epg_programs(id,channel_id,epg_channel_id,title,start_time,end_time) VALUES (5,3,'test','Saved',10000,20000),(6,3,'test','Latest',10000,20000); INSERT INTO recording_rules(id,user_id,title,created_at) VALUES (4,2,'Saved',0); INSERT INTO recordings(id,rule_id,channel_id,program_id,title,start_time,end_time,status,created_at,item_id,content_rating,output_path) VALUES (42,4,3,5,'Saved',9000,21000,'completed',0,7,'G','/data/recordings/42.ts');").execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../../mbl-db/migrations/0011_dvr_scheduling.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let row:(i64,i64,i64,i64,String)=sqlx::query_as("SELECT owner_id,library_id,item_id,program_start_time,content_rating FROM recordings WHERE id=42").fetch_one(&pool).await.unwrap();
    assert_eq!(row, (2, 1, 7, 10000, "G".to_owned()));
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM epg_programs").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    sqlx::query("DELETE FROM channels WHERE id=3").execute(&pool).await.unwrap();
    let channel: Option<i64> = sqlx::query_scalar("SELECT channel_id FROM recordings WHERE id=42")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(channel.is_none());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE id=7")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let violations = sqlx::query("PRAGMA foreign_key_check").fetch_all(&pool).await.unwrap();
    assert!(violations.is_empty());
    sqlx::query("DELETE FROM recordings WHERE id=42").execute(&pool).await.unwrap();
    let next:i64=sqlx::query_scalar("INSERT INTO recordings(channel_id,title,start_time,end_time,created_at,program_start_time,program_end_time) VALUES (NULL,'Next',1,2,0,1,2) RETURNING id").fetch_one(&pool).await.unwrap();
    assert!(next > 42);
}
