#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use mbl_probe::FfTools;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

async fn cache_probe(db: &mbl_db::Db, tools: &FfTools, path: &str, part: i64) {
    let info = tools.probe(Path::new(path), Duration::from_secs(15)).await.unwrap();
    sqlx::query("UPDATE media_parts SET probe=?,duration_ms=? WHERE id=?")
        .bind(serde_json::to_string(&info).unwrap())
        .bind(info.duration_ms as i64)
        .bind(part)
        .execute(&db.write)
        .await
        .unwrap();
}
async fn playlist(services: &mbl_service::Services, response: &Value) -> std::path::PathBuf {
    let session = services.stream.hls_session(response["sessionId"].as_str().unwrap()).unwrap();
    session.request_segment("0", 0).await.unwrap();
    let path = session.dir.join("0/ffmpeg.m3u8").into_std_path_buf();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if tokio::fs::read_to_string(&path)
                .await
                .is_ok_and(|text| text.contains("#EXT-X-ENDLIST"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    path
}
async fn stop(app: &TestApp, admin: &str, response: &Value) {
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", response["sessionId"].as_str().unwrap()),
        Some(admin),
        None,
    )
    .await;
}

#[tokio::test]
async fn selected_audio_and_ass_subtitles_are_real_and_invalid_indices_are_rejected() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let path: String = sqlx::query_scalar("SELECT path FROM media_parts WHERE id=?")
        .bind(seeded.movie_part_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let caption = app.dir.path().join("caption.srt");
    std::fs::write(&caption, "1\n00:00:00,000 --> 00:00:02,500\nHello tracks\n").unwrap();
    let tools = FfTools::locate("", "").await.unwrap();
    let output = tokio::process::Command::new(&tools.ffmpeg)
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=320x180:r=24:d=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=3",
            "-i",
            caption.to_str().unwrap(),
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:s",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "24",
            "-c:a",
            "aac",
            "-c:s",
            "ass",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=swe",
            "-metadata:s:a:1",
            "title=Swedish",
            "-disposition:a:0",
            "default",
            "-disposition:a:1",
            "0",
            "-f",
            "matroska",
        ])
        .arg(&path)
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    cache_probe(&db, &tools, &path, seeded.movie_part_id).await;
    let start=app.call(Method::POST,"/api/v1/playback",Some(&admin),Some(json!({"itemId":seeded.movie_id,"profile":"chrome","audioStreamIndex":2,"subtitleStreamIndex":3}))).await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    assert_eq!(start.body["audioStreamIndex"], 2);
    assert_eq!(start.body["audioTracks"].as_array().unwrap().len(), 2);
    assert_eq!(start.body["audioTracks"][1]["language"], "swe");
    assert!(start.body["burnedSubtitleStreamIndex"].is_null());
    assert_eq!(start.body["subtitles"][0]["format"], "vtt");
    assert_eq!(start.body["subtitleOptions"][0]["requiresTranscode"], false);
    let subtitle = app
        .call(Method::GET, start.body["subtitles"][0]["url"].as_str().unwrap(), None, None)
        .await;
    assert_eq!(subtitle.status, StatusCode::OK, "{}", subtitle.body);
    assert!(subtitle.body.as_str().unwrap().contains("WEBVTT"));
    assert!(subtitle.body.as_str().unwrap().contains("Hello tracks"));
    let hls = playlist(&services, &start.body).await;
    let decoded = tokio::process::Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&hls)
        .args(["-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "f32le", "pipe:1"])
        .output()
        .await
        .unwrap();
    assert!(decoded.status.success(), "{}", String::from_utf8_lossy(&decoded.stderr));
    let samples: Vec<f32> =
        decoded.stdout.as_chunks::<4>().0.iter().map(|bytes| f32::from_le_bytes(*bytes)).collect();
    let crossings = samples[4800..14400].windows(2).filter(|s| s[0] <= 0.0 && s[1] > 0.0).count();
    let frequency = crossings as f64 / 0.2;
    assert!(
        (850.0..910.0).contains(&frequency),
        "selected stream should contain 880Hz, got {frequency}"
    );
    let diagnostics = app
        .call(
            Method::GET,
            &format!("/api/v1/playback/{}/diagnostics", start.body["sessionId"].as_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(diagnostics.body["audioStreamIndex"], 2);
    for request in [
        json!({"audioStreamIndex":3}),
        json!({"audioStreamIndex":999}),
        json!({"subtitleStreamIndex":1}),
        json!({"subtitleStreamIndex":999}),
    ] {
        let mut body = request;
        body["itemId"] = json!(seeded.movie_id);
        assert_eq!(
            app.call(Method::POST, "/api/v1/playback", Some(&admin), Some(body.clone()))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            app.call(
                Method::POST,
                &format!("/api/v1/items/{}/playback-preview", seeded.movie_id),
                Some(&admin),
                Some(body)
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    stop(&app, &admin, &start.body).await;
    // A profile requiring burn-in must expose that fact and really render the caption.
    let profile = json!({"name":"chrome","subtitles":[{"format":"ass","method":"burn"}]});
    let burned=app.call(Method::POST,"/api/v1/playback",Some(&admin),Some(json!({"itemId":seeded.movie_id,"profile":profile,"audioStreamIndex":2,"subtitleStreamIndex":3}))).await;
    assert_eq!(burned.status, StatusCode::OK, "{}", burned.body);
    assert_eq!(burned.body["burnedSubtitleStreamIndex"], 3);
    assert_eq!(burned.body["subtitleOptions"][0]["requiresTranscode"], true);
    assert!(burned.body["subtitles"].as_array().unwrap().is_empty());
    let hls = playlist(&services, &burned.body).await;
    let picture = tokio::process::Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(hls)
        .args([
            "-ss",
            "1",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(picture.status.success(), "{}", String::from_utf8_lossy(&picture.stderr));
    assert!(
        picture.stdout.iter().any(|pixel| *pixel > 100),
        "caption must be visible on the black frame (max={:?}, bytes={})",
        picture.stdout.iter().max(),
        picture.stdout.len()
    );
    stop(&app, &admin, &burned.body).await;
    let off = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id,"profile":"chrome","audioStreamIndex":2})),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK);
    assert!(off.body["burnedSubtitleStreamIndex"].is_null());
    assert_eq!(off.body["audioStreamIndex"], 2);
    stop(&app, &admin, &off.body).await;
    // External text tracks use their own validated file, not an index into the movie.
    let sidecar = Path::new(&path).parent().unwrap().join("Heat.en.srt");
    std::fs::copy(&caption, &sidecar).unwrap();
    let mut info = tools.probe(Path::new(&path), Duration::from_secs(10)).await.unwrap();
    let mut external =
        info.streams.iter().find(|s| s.kind == mbl_core::StreamKind::Subtitle).unwrap().clone();
    external.index = 4;
    external.codec = "subrip".into();
    external.external_path = Some(sidecar.to_string_lossy().into_owned());
    info.streams.push(external);
    sqlx::query("UPDATE media_parts SET probe=? WHERE id=?")
        .bind(serde_json::to_string(&info).unwrap())
        .bind(seeded.movie_part_id)
        .execute(&db.write)
        .await
        .unwrap();
    let external = app.call(Method::POST,"/api/v1/playback",Some(&admin),Some(json!({"itemId":seeded.movie_id,"profile":{"name":"chrome","subtitles":[{"format":"subrip","method":"burn"}]},"audioStreamIndex":2,"subtitleStreamIndex":4}))).await;
    assert_eq!(external.status, StatusCode::OK, "{}", external.body);
    assert_eq!(external.body["burnedSubtitleStreamIndex"], 4);
    let external_hls = playlist(&services, &external.body).await;
    let picture = tokio::process::Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(external_hls)
        .args([
            "-ss",
            "1",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(picture.status.success());
    assert!(picture.stdout.iter().any(|pixel| *pixel > 100));
    stop(&app, &admin, &external.body).await;
    // A profile asking for burn-in must still not read outside a library root.
    info.streams.iter_mut().find(|s| s.index == 4).unwrap().external_path =
        Some(caption.to_string_lossy().into_owned());
    sqlx::query("UPDATE media_parts SET probe=? WHERE id=?")
        .bind(serde_json::to_string(&info).unwrap())
        .bind(seeded.movie_part_id)
        .execute(&db.write)
        .await
        .unwrap();
    let outside = app.call(Method::POST,"/api/v1/playback",Some(&admin),Some(json!({"itemId":seeded.movie_id,"profile":{"name":"chrome","subtitles":[{"format":"subrip","method":"burn"}]},"subtitleStreamIndex":4}))).await;
    assert_ne!(outside.status, StatusCode::OK);
}

#[tokio::test]
async fn video_without_audio_transcodes_without_mapping_a_nonexistent_stream() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let path: String = sqlx::query_scalar("SELECT path FROM media_parts WHERE id=?")
        .bind(seeded.movie_part_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let tools = FfTools::locate("", "").await.unwrap();
    assert!(
        tokio::process::Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=64x36:r=24:d=2",
                "-c:v",
                "mpeg4",
                "-an"
            ])
            .arg(&path)
            .status()
            .await
            .unwrap()
            .success()
    );
    cache_probe(&db, &tools, &path, seeded.movie_part_id).await;
    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":seeded.movie_id,"profile":"chrome"})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    assert_eq!(start.body["method"], "transcode");
    assert!(start.body["audioStreamIndex"].is_null());
    assert!(start.body["audioTracks"].as_array().unwrap().is_empty());
    let path = playlist(&services, &start.body).await;
    let info = tools.probe(&path, Duration::from_secs(10)).await.unwrap();
    assert!(info.streams.iter().all(|s| s.kind != mbl_core::StreamKind::Audio));
    assert!((1800..=2300).contains(&info.duration_ms));
    let master = app.call(Method::GET, start.body["url"].as_str().unwrap(), None, None).await;
    assert!(!master.body.as_str().unwrap().contains("mp4a"));
    stop(&app, &admin, &start.body).await;
}
