#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_core::{ChannelId, UserId, time::now_ms};
use mbl_db::{Db, repo::m9};
use serde_json::json;
use std::time::Duration;

async fn fixture(
    source: &str,
) -> (TestApp, Db, mbl_service::Services, String, String, i64, UserId) {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let profile = app.create_user(&admin, "LiveProfile").await;
    let user = services.auth.authenticate(&profile).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET content_rating_ceiling='PG',is_managed=1 WHERE id=?")
        .bind(user.0)
        .execute(&db.write)
        .await
        .unwrap();
    let imported=app.call(Method::POST,"/api/v1/livetv/tuners/m3u",Some(&admin),Some(json!({"name":"Live tests","content":format!("#EXTM3U\n#EXTINF:-1 tvg-id=\"profile\",Profile channel\n{source}\n")}))).await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.body);
    let channel =
        sqlx::query_scalar("SELECT id FROM channels LIMIT 1").fetch_one(&db.read).await.unwrap();
    (app, db, services, admin, profile, channel, user)
}
async fn programme(
    db: &Db,
    channel: i64,
    title: &str,
    start: i64,
    end: i64,
    rating: Option<&str>,
) -> i64 {
    m9::batch_upsert_epg_programs(
        db,
        &[m9::EpgProgramInsert {
            channel_id: Some(ChannelId(channel)),
            epg_channel_id: "profile".into(),
            title: title.into(),
            description: Some(format!("{title} description")),
            season_number: None,
            episode_number: None,
            start_time: start,
            end_time: end,
            icon_url: None,
            rating: rating.map(str::to_owned),
            genres: vec![],
            is_new: false,
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
async fn start(app: &TestApp, token: &str, channel: i64) -> common::Res {
    app.call(Method::GET, &format!("/api/v1/livetv/stream/{channel}"), Some(token), None).await
}
fn idle(services: &mbl_service::Services) {
    assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
}

#[tokio::test]
async fn profile_filters_guide_metadata_and_sources_before_applying_search_limit() {
    let (app, db, services, admin, profile, channel, _) =
        fixture("http://provider.invalid/private-password").await;
    let now = now_ms();
    let adult =
        programme(&db, channel, "Secret adult current", now - 1000, now + 60000, Some("R")).await;
    for n in 0..205 {
        programme(
            &db,
            channel,
            "Search adult",
            now + 60000 + n * 1000,
            now + 61000 + n * 1000,
            Some("R"),
        )
        .await;
    }
    programme(&db, channel, "Search allowed", now + 500000, now + 501000, Some("PG")).await;
    let channels = app.call(Method::GET, "/api/v1/livetv/channels", Some(&profile), None).await;
    assert_eq!(channels.status, StatusCode::OK);
    assert_eq!(channels.body[0]["canPlay"], false);
    assert_eq!(channels.body[0]["streamUrl"], "");
    assert!(channels.body[0].get("currentProgram").is_none());
    let detail = app
        .call(Method::GET, &format!("/api/v1/livetv/channels/{channel}"), Some(&profile), None)
        .await;
    assert!(!detail.body.to_string().contains("Secret adult"));
    let grid = app
        .call(
            Method::GET,
            &format!("/api/v1/livetv/epg?startTime={now}&endTime={}", now + 1000000),
            Some(&profile),
            None,
        )
        .await;
    assert!(!grid.body.to_string().contains("adult"));
    assert!(!grid.body.to_string().contains("private-password"));
    let search = app
        .call(Method::GET, "/api/v1/livetv/epg/search?query=Search&limit=1", Some(&profile), None)
        .await;
    assert_eq!(search.status, StatusCode::OK, "{}", search.body);
    assert_eq!(search.body[0]["title"], "Search allowed");
    let tuners = app.call(Method::GET, "/api/v1/livetv/tuners", Some(&profile), None).await;
    let tuner = tuners.body[0]["id"].as_i64().unwrap();
    assert_eq!(tuners.body[0]["url"], "");
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/livetv/tuners/{tuner}"), Some(&profile), None)
            .await
            .body["url"],
        ""
    );
    assert_eq!(start(&app, &profile, channel).await.status, StatusCode::FORBIDDEN);
    idle(&services);
    sqlx::query("DELETE FROM epg_programs WHERE id=?")
        .bind(adult)
        .execute(&db.write)
        .await
        .unwrap();
    assert_eq!(start(&app, &profile, channel).await.status, StatusCode::FORBIDDEN);
    idle(&services);
    let actor = services.auth.authenticate(&profile).await.unwrap().actor;
    let caller = mbl_mcp::Caller {
        actor,
        scopes: mbl_auth::tokens::ScopeSet::full_for_user(false),
        client: "live-profile-test".into(),
        token_id: None,
    };
    let result = mbl_mcp::execute_tool(
        "search_guide",
        &json!({"query":"Search","limit":1}),
        &caller,
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap();
    let result = serde_json::to_value(result).unwrap();
    assert!(result.to_string().contains("Search allowed"));
    assert!(!result.to_string().contains("Search adult"));
    assert_eq!(
        app.call(Method::POST, "/api/v1/livetv/tuners/discover", Some(&profile), None).await.status,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE channels SET is_hidden=1 WHERE id=?")
        .bind(channel)
        .execute(&db.write)
        .await
        .unwrap();
    assert_eq!(
        app.call(Method::GET, "/api/v1/livetv/channels?includeHidden=true", Some(&profile), None)
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/livetv/channels/{channel}"), Some(&profile), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE channels SET is_hidden=0 WHERE id=?")
        .bind(channel)
        .execute(&db.write)
        .await
        .unwrap();
    let unrestricted = app.call(Method::GET, "/api/v1/livetv/channels", Some(&admin), None).await;
    assert!(unrestricted.body[0]["streamUrl"].as_str().unwrap().contains("private-password"));
}

#[tokio::test]
async fn rating_changes_revoke_media_renewal_and_background_leases() {
    let source = common::live_source().await;
    let (app, db, services, _admin, profile, channel, _) = fixture(&source.url).await;
    let now = now_ms();
    let id = programme(&db, channel, "Allowed", now - 1000, now + 120000, Some("G")).await;
    for via in ["media", "renew", "maintenance"] {
        sqlx::query("UPDATE epg_programs SET rating='G' WHERE id=?")
            .bind(id)
            .execute(&db.write)
            .await
            .unwrap();
        let playing = start(&app, &profile, channel).await;
        assert_eq!(playing.status, StatusCode::OK, "{}", playing.body);
        let lease = playing.body["sessionId"].as_str().unwrap();
        let url = playing.body["streamUrl"].as_str().unwrap();
        let path = services.livetv.live_file(lease, "init.mp4").await.unwrap();
        sqlx::query("UPDATE epg_programs SET rating='R' WHERE id=?")
            .bind(id)
            .execute(&db.write)
            .await
            .unwrap();
        match via {
            "media" => assert_eq!(
                app.call(Method::GET, url, None, None).await.status,
                StatusCode::FORBIDDEN
            ),
            "renew" => assert_eq!(
                app.call(
                    Method::GET,
                    &format!("/api/v1/livetv/stream/{channel}?sessionId={lease}"),
                    Some(&profile),
                    None
                )
                .await
                .status,
                StatusCode::FORBIDDEN
            ),
            _ => services.livetv.maintain().await,
        }
        idle(&services);
        assert!(!path.exists());
        assert!(services.livetv.live_file(lease, "init.mp4").await.is_err());
    }
}

#[tokio::test]
async fn programme_boundary_caps_fast_sources_and_requires_a_new_approved_lease() {
    let source = common::live_source().await;
    let (app, db, services, _admin, profile, channel, _) = fixture(&source.url).await;
    let now = now_ms();
    let end = now + 6000;
    programme(&db, channel, "First allowed", now - 1000, end, Some("G")).await;
    programme(&db, channel, "Next allowed", end, end + 120000, Some("G")).await;
    let playing = start(&app, &profile, channel).await;
    assert_eq!(playing.status, StatusCode::OK, "{}", playing.body);
    assert_eq!(playing.body["accessEndsAt"], end);
    assert_eq!(playing.body["expiresAt"], end);
    let url = playing.body["streamUrl"].as_str().unwrap();
    let lease = playing.body["sessionId"].as_str().unwrap();
    let path = services.livetv.live_file(lease, "index.m3u8").await.unwrap();
    // A fast finite HTTP source must not encode the entire 30-second source for a 6-second grant.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let playlist = tokio::fs::read_to_string(&path).await.unwrap();
            if playlist.contains("#EXT-X-ENDLIST") {
                let total: f64 = playlist
                    .lines()
                    .filter_map(|l| l.strip_prefix("#EXTINF:"))
                    .filter_map(|l| l.trim_end_matches(',').parse::<f64>().ok())
                    .sum();
                assert!(total <= 6.1, "encoded {total} seconds past bounded grant");
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis((end - now_ms()).max(0) as u64 + 30)).await;
    assert_eq!(app.call(Method::GET, url, None, None).await.status, StatusCode::NOT_FOUND);
    services.livetv.maintain().await;
    idle(&services);
    assert!(!path.exists());
    let next = start(&app, &profile, channel).await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.body);
    assert_ne!(next.body["sessionId"], playing.body["sessionId"]);
    services.livetv.shutdown().await;
    idle(&services);
}

#[tokio::test]
async fn logout_expiry_and_reused_session_ids_cannot_keep_capabilities_alive() {
    let source = common::live_source().await;
    for change in [
        "DELETE FROM sessions WHERE id=?",
        "UPDATE sessions SET expires_at=1 WHERE id=?",
        "UPDATE sessions SET token_hash=randomblob(32) WHERE id=?",
    ] {
        let (app, db, services, admin, _profile, channel, _) = fixture(&source.url).await;
        let playing = start(&app, &admin, channel).await;
        assert_eq!(playing.status, StatusCode::OK, "{}", playing.body);
        let owner = services.auth.authenticate(&admin).await.unwrap().session_id;
        sqlx::query(change).bind(owner.0).execute(&db.write).await.unwrap();
        let denied =
            app.call(Method::GET, playing.body["streamUrl"].as_str().unwrap(), None, None).await;
        assert_eq!(denied.status, StatusCode::UNAUTHORIZED);
        idle(&services);
    }
}

#[tokio::test]
async fn newly_restricted_profiles_cannot_reuse_unbounded_encoders() {
    let source = common::live_source().await;
    let (app, db, services, admin, _profile, channel, _) = fixture(&source.url).await;
    let now = now_ms();
    programme(&db, channel, "Allowed", now - 1000, now + 120000, Some("G")).await;
    let playing = start(&app, &admin, channel).await;
    assert_eq!(playing.status, StatusCode::OK, "{}", playing.body);
    assert!(playing.body.get("accessEndsAt").is_none());
    let owner = services.auth.authenticate(&admin).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET is_admin=0,content_rating_ceiling='PG' WHERE id=?")
        .bind(owner.0)
        .execute(&db.write)
        .await
        .unwrap();
    assert_eq!(
        app.call(Method::GET, playing.body["streamUrl"].as_str().unwrap(), None, None).await.status,
        StatusCode::FORBIDDEN
    );
    idle(&services);
    let next = start(&app, &admin, channel).await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.body);
    assert_eq!(next.body["accessEndsAt"], now + 120000);
    services.livetv.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn policy_changes_during_preparation_abort_the_candidate_and_reap_its_child() {
    use std::os::unix::fs::PermissionsExt;
    let (app, db, services, _admin, profile, channel, user) =
        fixture("http://127.0.0.1:9/unused").await;
    let now = now_ms();
    programme(&db, channel, "PG programme", now - 1000, now + 120000, Some("PG")).await;
    let script = app.dir.path().join("stall.sh");
    std::fs::write(&script,"#!/bin/sh\nfor last; do true; done\nout_dir=$(dirname \"$last\")\necho $$ > \"$out_dir/pid\"\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let service =
        services.livetv.clone().with_stream(services.stream.clone().with_ffmpeg_bin(script));
    let pending = service.clone();
    let owner = services.auth.authenticate(&profile).await.unwrap().session_id;
    let task =
        tokio::spawn(
            async move { pending.resolve_live_stream(owner, ChannelId(channel), None).await },
        );
    let root = services.stream.hls_manager().unwrap().config().transcode_dir.join("live");
    let directory = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(entries) = std::fs::read_dir(&root)
                && let Some(entry) = entries.flatten().find(|e| e.path().join("pid").exists())
            {
                break entry.path();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    sqlx::query("UPDATE users SET content_rating_ceiling='G' WHERE id=?")
        .bind(user.0)
        .execute(&db.write)
        .await
        .unwrap();
    tokio::fs::write(directory.join("init.mp4"), b"init").await.unwrap();
    tokio::fs::write(directory.join("seg_000000000.m4s"), b"media").await.unwrap();
    tokio::fs::write(directory.join("index.m3u8"), b"#EXTM3U\n#EXTINF:2,\nseg_000000000.m4s\n")
        .await
        .unwrap();
    assert!(matches!(task.await.unwrap(), Err(mbl_core::Error::Forbidden)));
    idle(&services);
    assert!(!directory.exists());
}
