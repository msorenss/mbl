#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use mbl_core::{Actor, PartId, UserId};
use serde_json::json;
use tokio_util::sync::CancellationToken;

async fn make_video(path: &str, color: &str) {
    assert!(
        tokio::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!("color={color}:s=64x36:r=1:d=11"),
                "-c:v",
                "mpeg4",
                "-g",
                "1"
            ])
            .arg(path)
            .status()
            .await
            .unwrap()
            .success()
    );
}
async fn ready(app: &TestApp, admin: &str, index: &str) -> common::Res {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let response = app.call(Method::GET, index, Some(admin), None).await;
            if response.status != StatusCode::ACCEPTED {
                return response;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn queued_previews_generate_real_images_and_invalidate_stale_sources() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mbl_server::Config::default();
    config.paths.data_dir = dir.path().join("data").to_string_lossy().into_owned();
    config.scanner.watch = false;
    let server = mbl_server::build(config).await.unwrap();
    let db = server.db.clone();
    let services = server.services.clone();
    let app = TestApp { router: server.router.clone(), dir };
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let part = seeded.movie_part_id;
    let path: String = sqlx::query_scalar("SELECT path FROM media_parts WHERE id=?")
        .bind(part)
        .fetch_one(&db.read)
        .await
        .unwrap();
    make_video(&path, "red").await;
    sqlx::query("UPDATE media_parts SET duration_ms=11000 WHERE id=?")
        .bind(part)
        .execute(&db.write)
        .await
        .unwrap();
    // Old successful-looking placeholder caches must not be served after the upgrade.
    sqlx::query("INSERT INTO trickplay(part_id,frame_count,sprite_count,manifest_json,created_at) VALUES (?,2,1,?,0)")
        .bind(part).bind(serde_json::to_string(&mbl_analysis::build_manifest(PartId(part),11000,1)).unwrap()).execute(&db.write).await.unwrap();
    let index = format!("/api/v1/stream/trickplay/{part}/index.json");
    let sprite = format!("/api/v1/stream/trickplay/{part}/sprite_000.jpg");
    assert_eq!(app.call(Method::GET, &index, None, None).await.status, StatusCode::UNAUTHORIZED);
    let (first, second) = tokio::join!(
        app.call(Method::GET, &index, Some(&admin), None),
        app.call(Method::GET, &index, Some(&admin), None)
    );
    assert_eq!(first.status, StatusCode::ACCEPTED);
    assert_eq!(second.status, StatusCode::ACCEPTED);
    assert_eq!(first.headers["retry-after"], "5");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind='media.trickplay'")
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(count, 1, "simultaneous index requests must enqueue one job");
    assert_eq!(
        app.call(Method::GET, &sprite, Some(&admin), None).await.status,
        StatusCode::NOT_FOUND
    );
    // Idle work stays queued while playback is active.
    let playing = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id})),
        )
        .await;
    assert_eq!(playing.status, StatusCode::OK);
    let stop = CancellationToken::new();
    let _running = server.start_jobs(stop.clone()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE kind='media.trickplay'")
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(status, "queued");
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", playing.body["sessionId"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    let response = ready(&app, &admin, &index).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.body["frameCount"], 2);
    let jpeg = app.call(Method::GET, &sprite, Some(&admin), None).await;
    assert_eq!(jpeg.status, StatusCode::OK);
    assert_eq!(jpeg.headers["content-type"], "image/jpeg");
    assert_eq!(jpeg.headers["cache-control"], "private, no-store");
    let actor = Actor::admin(UserId(1));
    let red = services
        .intelligence
        .trickplay_sprite_path(&actor, PartId(part), "sprite_000.jpg")
        .await
        .unwrap();
    assert!(image::open(&red).unwrap().to_rgb8().get_pixel(160, 90)[0] > 180);
    // A missing artifact is never reported as a ready manifest.
    std::fs::remove_file(&red).unwrap();
    assert_eq!(
        app.call(Method::GET, &index, Some(&admin), None).await.status,
        StatusCode::ACCEPTED
    );
    assert_eq!(ready(&app, &admin, &index).await.status, StatusCode::OK);
    make_video(&path, "blue").await;
    assert_eq!(
        app.call(Method::GET, &index, Some(&admin), None).await.status,
        StatusCode::ACCEPTED
    );
    assert_eq!(ready(&app, &admin, &index).await.status, StatusCode::OK);
    let blue = services
        .intelligence
        .trickplay_sprite_path(&actor, PartId(part), "sprite_000.jpg")
        .await
        .unwrap();
    assert_ne!(red, blue);
    assert!(image::open(blue).unwrap().to_rgb8().get_pixel(160, 90)[2] > 180);
    stop.cancel();
}

#[tokio::test]
async fn preview_access_checks_rating_library_and_source_root_before_cache_or_generation() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let guest = app.create_user(&admin, "guest").await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    sqlx::query("UPDATE users SET content_rating_ceiling='PG' WHERE username='guest'")
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE media_items SET content_rating='R' WHERE id=?")
        .bind(seeded.movie_id)
        .execute(&db.write)
        .await
        .unwrap();
    for file in ["index.json", "sprite_000.jpg"] {
        let uri = format!("/api/v1/stream/trickplay/{}/{file}", seeded.movie_part_id);
        assert_eq!(
            app.call(Method::GET, &uri, Some(&guest), None).await.status,
            StatusCode::NOT_FOUND
        );
    }
    sqlx::query("UPDATE media_items SET content_rating='PG' WHERE id=?")
        .bind(seeded.movie_id)
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_managed=1 WHERE username='guest'")
        .execute(&db.write)
        .await
        .unwrap();
    let uri = format!("/api/v1/stream/trickplay/{}/index.json", seeded.movie_part_id);
    assert_eq!(app.call(Method::GET, &uri, Some(&guest), None).await.status, StatusCode::NOT_FOUND);
    let outside = app.dir.path().join("outside.mp4");
    std::fs::write(&outside, b"outside").unwrap();
    sqlx::query("UPDATE media_parts SET path=? WHERE id=?")
        .bind(outside.to_str().unwrap())
        .bind(seeded.movie_part_id)
        .execute(&db.write)
        .await
        .unwrap();
    assert!(
        services.intelligence.generate_trickplay_part(PartId(seeded.movie_part_id)).await.is_err()
    );
    let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind='media.trickplay'")
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(jobs, 0);
}
