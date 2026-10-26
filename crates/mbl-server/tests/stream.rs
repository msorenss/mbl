#![allow(clippy::unwrap_used)]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use http_body_util::BodyExt;
use mbl_core::ids::PartId;
use serde_json::json;
use tower::ServiceExt;

struct SeededStream {
    lib_id: i64,
    part_id: i64,
    payload: Vec<u8>,
}

async fn seed_part(app: &TestApp, db: &mbl_db::Db, admin: &str) -> SeededStream {
    let movies_dir = app.media_dir("Movies");
    let lib = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(admin),
            Some(json!({ "name": "Movies", "kind": "movie", "paths": [movies_dir] })),
        )
        .await;
    assert_eq!(lib.status, StatusCode::CREATED);
    let lib_id = lib.body["id"].as_i64().unwrap();

    // Deterministic 1024-byte payload so slices can be verified byte-for-byte.
    let payload: Vec<u8> = (0..1024u32).map(|i| (i % 251) as u8).collect();
    let file_path = std::path::Path::new(&movies_dir).join("Sample (2024).mp4");
    std::fs::write(&file_path, &payload).unwrap();

    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, year, added_at, updated_at)
         VALUES (?, 'movie', 'Sample', 'sample', 2024, 1, 1) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let version_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, bitrate, width, height, video_codec, audio_codec, audio_channels)
         VALUES (?, 'mov,mp4,m4a,3gp,3g2,mj2', 800000, 1920, 1080, 'h264', 'aac', 2) RETURNING id",
    )
    .bind(item_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let part_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, probe, chapters, part_index)
         VALUES (?, ?, ?, 1, 10000, '{}', '[]', 0) RETURNING id",
    )
    .bind(version_id)
    .bind(file_path.to_str().unwrap())
    .bind(payload.len() as i64)
    .fetch_one(&db.write)
    .await
    .unwrap();

    SeededStream { lib_id, part_id, payload }
}

async fn raw_request(
    app: &TestApp,
    method: Method,
    uri: &str,
    auth_header: Option<&str>,
    cookie_header: Option<&str>,
    range_header: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = auth_header {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(cookie) = cookie_header {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(range) = range_header {
        builder = builder.header(header::RANGE, range);
    }
    let req = builder.body(Body::empty()).unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, bytes)
}

#[tokio::test]
async fn direct_play_full_head_and_byte_ranges() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_part(&app, &db, &admin).await;
    let uri = format!("/api/v1/stream/direct/{}", seeded.part_id);

    // 1. Full GET -> 200 OK
    let (status, headers, bytes) =
        raw_request(&app, Method::GET, &uri, Some(&admin), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::ACCEPT_RANGES).unwrap(), "bytes");
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "video/mp4");
    assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "1024");
    assert_eq!(bytes, seeded.payload);

    // 2. HEAD -> 200 OK, Content-Length = 1024, empty body
    let (status, headers, bytes) =
        raw_request(&app, Method::HEAD, &uri, Some(&admin), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::ACCEPT_RANGES).unwrap(), "bytes");
    assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "1024");
    assert!(bytes.is_empty());

    // 3. Start range: bytes=0-127
    let (status, headers, bytes) =
        raw_request(&app, Method::GET, &uri, Some(&admin), None, Some("bytes=0-127")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers.get(header::CONTENT_RANGE).unwrap(), "bytes 0-127/1024");
    assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "128");
    assert_eq!(bytes, &seeded.payload[0..=127]);

    // 4. Middle range: bytes=100-299
    let (status, headers, bytes) =
        raw_request(&app, Method::GET, &uri, Some(&admin), None, Some("bytes=100-299")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers.get(header::CONTENT_RANGE).unwrap(), "bytes 100-299/1024");
    assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "200");
    assert_eq!(bytes, &seeded.payload[100..=299]);

    // 5. Open-ended range: bytes=900-
    let (status, headers, bytes) =
        raw_request(&app, Method::GET, &uri, Some(&admin), None, Some("bytes=900-")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers.get(header::CONTENT_RANGE).unwrap(), "bytes 900-1023/1024");
    assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "124");
    assert_eq!(bytes, &seeded.payload[900..1024]);

    // 6. Suffix range: bytes=-150
    let (status, headers, bytes) =
        raw_request(&app, Method::GET, &uri, Some(&admin), None, Some("bytes=-150")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers.get(header::CONTENT_RANGE).unwrap(), "bytes 874-1023/1024");
    assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "150");
    assert_eq!(bytes, &seeded.payload[874..1024]);

    // 7. HEAD with Range -> 206 Partial Content, empty body
    let (status, headers, bytes) =
        raw_request(&app, Method::HEAD, &uri, Some(&admin), None, Some("bytes=100-199")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers.get(header::CONTENT_RANGE).unwrap(), "bytes 100-199/1024");
    assert_eq!(headers.get(header::CONTENT_LENGTH).unwrap(), "100");
    assert!(bytes.is_empty());

    // 8. Invalid ranges -> 416 Range Not Satisfiable
    for bad_range in
        ["bytes=1024-2000", "bytes=500-200", "bytes=0-10, 20-30", "bytes=-0", "items=0-5"]
    {
        let (status, headers, _) =
            raw_request(&app, Method::GET, &uri, Some(&admin), None, Some(bad_range)).await;
        assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE, "range: {bad_range}");
        assert_eq!(headers.get(header::CONTENT_RANGE).unwrap(), "bytes */1024");
    }
}

#[tokio::test]
async fn direct_play_auth_cookie_query_token_and_acl() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_part(&app, &db, &admin).await;
    let uri = format!("/api/v1/stream/direct/{}", seeded.part_id);

    // Unauthenticated -> 401
    let (status, _, _) = raw_request(&app, Method::GET, &uri, None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Cookie authentication -> 200
    let cookie = format!("mbl_session={admin}");
    let (status, _, bytes) =
        raw_request(&app, Method::GET, &uri, None, Some(&cookie), Some("bytes=0-15")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(bytes, &seeded.payload[0..16]);

    // Query ?token= authentication -> 200
    let admin_user = services.auth.authenticate(&admin).await.unwrap();
    let stream_tok = services.auth.sign_stream_token(&admin_user.actor, PartId(seeded.part_id));
    let tok_uri = format!("{uri}?token={stream_tok}");
    let (status, _, bytes) =
        raw_request(&app, Method::GET, &tok_uri, None, None, Some("bytes=0-31")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(bytes, &seeded.payload[0..32]);

    // Query ?token= signed for a different partId -> 401
    let wrong_part_tok =
        services.auth.sign_stream_token(&admin_user.actor, PartId(seeded.part_id + 999));
    let wrong_uri = format!("{uri}?token={wrong_part_tok}");
    let (status, _, _) = raw_request(&app, Method::GET, &wrong_uri, None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Expired ?token= -> 401
    let expired_tok = services.auth.sign_stream_token_with_ttl(
        admin_user.actor.user_id,
        PartId(seeded.part_id),
        -1_000,
    );
    let expired_uri = format!("{uri}?token={expired_tok}");
    let (status, _, _) = raw_request(&app, Method::GET, &expired_uri, None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Non-existent partId -> 404
    let (status, _, _) =
        raw_request(&app, Method::GET, "/api/v1/stream/direct/999999", Some(&admin), None, None)
            .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Restricted Actor without library access -> Error::NotFound; with access -> Ok
    let alice_tok = app.create_user(&admin, "alice").await;
    let alice_user = services.auth.authenticate(&alice_tok).await.unwrap();
    let restricted_actor = mbl_core::Actor {
        user_id: alice_user.actor.user_id,
        is_admin: false,
        library_access: mbl_core::LibraryAccess::Only(vec![]),
        is_managed: false,
        content_rating_ceiling: None,
    };
    let denied = services.stream.resolve_part(&restricted_actor, PartId(seeded.part_id)).await;
    assert!(matches!(denied, Err(mbl_core::Error::NotFound)));

    let granted_actor = mbl_core::Actor {
        user_id: alice_user.actor.user_id,
        is_admin: false,
        library_access: mbl_core::LibraryAccess::Only(vec![mbl_core::LibraryId(seeded.lib_id)]),
        is_managed: false,
        content_rating_ceiling: None,
    };
    let resolved =
        services.stream.resolve_part(&granted_actor, PartId(seeded.part_id)).await.unwrap();
    assert_eq!(resolved.id, PartId(seeded.part_id));
}

#[tokio::test]
async fn direct_play_rejects_path_traversal_outside_library_root() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_part(&app, &db, &admin).await;

    // Create a secret file outside the library root.
    let outside_file = app.dir.path().join("outside-secret.mp4");
    std::fs::write(&outside_file, b"top secret bytes").unwrap();

    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'movie', 'Rogue', 'rogue', 1, 1) RETURNING id",
    )
    .bind(seeded.lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let version_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container)
         VALUES (?, 'mp4') RETURNING id",
    )
    .bind(item_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    // 1. Direct path outside library root via `..` segments
    let movies_dir = app.media_dir("Movies");
    let dotdot_path = format!("{movies_dir}/../../outside-secret.mp4");
    let dotdot_part_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, probe, chapters, part_index)
         VALUES (?, ?, 16, 1, '{}', '[]', 0) RETURNING id",
    )
    .bind(version_id)
    .bind(&dotdot_path)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let (status, _, _) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/direct/{dotdot_part_id}"),
        Some(&admin),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 2. Symlink inside library root pointing outside library root
    #[cfg(unix)]
    {
        let symlink_path = std::path::Path::new(&movies_dir).join("escape-link.mp4");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();
        let symlink_part_id: i64 = sqlx::query_scalar(
            "INSERT INTO media_parts (version_id, path, size, mtime, probe, chapters, part_index)
             VALUES (?, ?, 16, 1, '{}', '[]', 1) RETURNING id",
        )
        .bind(version_id)
        .bind(symlink_path.to_str().unwrap())
        .fetch_one(&db.write)
        .await
        .unwrap();

        let (status, _, _) = raw_request(
            &app,
            Method::GET,
            &format!("/api/v1/stream/direct/{symlink_part_id}"),
            Some(&admin),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn hls_endpoints_serve_playlists_init_and_segments_and_seek() {
    let Ok(tools) = mbl_probe::FfTools::locate("", "").await else {
        eprintln!("skipping: ffmpeg not available");
        return;
    };
    let (app, _db, services) = TestApp::new_with_services().await;
    let _admin = app.setup_admin().await;

    let media_file = app.dir.path().join("hls_source_12s.mkv");
    let gen_status = tokio::process::Command::new(&tools.ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=12:size=320x180:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=12",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "24",
            "-c:a",
            "aac",
            media_file.to_str().unwrap(),
        ])
        .status()
        .await
        .unwrap();
    assert!(gen_status.success());

    let hls_mgr = services.stream.hls_manager().unwrap();
    let master = mbl_stream::hls::render_master_playlist(&[mbl_stream::hls::HlsRendition {
        name: "0".to_owned(),
        width: Some(320),
        height: Some(180),
        bandwidth: 600_000,
        video_codec: Some("h264".to_owned()),
        audio_codec: Some("aac".to_owned()),
    }]);
    let media_m3u8 = mbl_stream::hls::render_media_playlist(12_000, 2);

    let session = hls_mgr
        .create_session(mbl_transcode::CreateSessionReq {
            user_id: mbl_core::UserId(1),
            part_id: PartId(1),
            duration_ms: 12_000,
            pipeline: mbl_transcode::Pipeline {
                input: camino::Utf8PathBuf::from_path_buf(media_file).unwrap(),
                start_ms: 0,
                video: mbl_transcode::VideoOp::Copy,
                audio: mbl_transcode::AudioOp::Copy { index: 1 },
                subtitle: mbl_transcode::SubtitleOp::None,
                output: mbl_transcode::HlsOutput {
                    dir: camino::Utf8PathBuf::from("/tmp/unused"),
                    segment_secs: 2,
                    start_number: 0,
                    renditions: vec![mbl_transcode::Rendition {
                        name: "0".to_owned(),
                        width: Some(320),
                        max_bitrate: 600_000,
                    }],
                },
                backend: mbl_transcode::Backend::Software,
            },
            master_playlist: master.clone(),
            media_playlists: vec![("0".to_owned(), media_m3u8.clone())],
            fast_start_segments: false,
        })
        .await
        .unwrap();

    let sid = &session.id;

    // 1. Master playlist
    let (status, headers, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/hls/{sid}/master.m3u8"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/vnd.apple.mpegurl");
    assert_eq!(String::from_utf8_lossy(&bytes), master);

    // 2. Media playlist
    let (status, headers, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/hls/{sid}/0/index.m3u8"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/vnd.apple.mpegurl");
    assert_eq!(String::from_utf8_lossy(&bytes), media_m3u8);

    // 3. init.mp4
    let (status, headers, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/hls/{sid}/0/init.mp4"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "video/mp4");
    assert!(!bytes.is_empty());

    // 4. Segment 0.m4s
    let (status, headers, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/hls/{sid}/0/0.m4s"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "video/iso.segment");
    assert!(!bytes.is_empty());

    // 5. Non-existent session -> 404
    let (status, _, _) = raw_request(
        &app,
        Method::GET,
        "/api/v1/stream/hls/nonexistent/master.m3u8",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    hls_mgr.shutdown_all().await;
}

#[tokio::test]
async fn subtitle_endpoint_converts_embedded_and_sidecar_and_serves_ass() {
    let Ok(tools) = mbl_probe::FfTools::locate("", "").await else {
        eprintln!("skipping: ffmpeg not available");
        return;
    };
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_part(&app, &db, &admin).await;

    let movies_dir = std::path::PathBuf::from(app.media_dir("Movies"));
    let embedded_srt_src = movies_dir.join("embedded_source.srt");
    std::fs::write(
        &embedded_srt_src,
        b"1\n00:00:01,000 --> 00:00:02,000\nEmbedded subtitle line\n",
    )
    .unwrap();

    let sidecar_srt = movies_dir.join("Movie.en.srt");
    std::fs::write(&sidecar_srt, b"1\n00:00:02,000 --> 00:00:03,000\nSidecar SRT line\n").unwrap();

    let sidecar_ass = movies_dir.join("Movie.jpn.ass");
    let raw_ass = "[Script Info]\nTitle: Test ASS\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Raw ASS dialogue\n";
    std::fs::write(&sidecar_ass, raw_ass.as_bytes()).unwrap();

    let mkv_file = movies_dir.join("Movie.mkv");
    let mux_status = tokio::process::Command::new(&tools.ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=4:size=320x180:rate=24",
            "-i",
            embedded_srt_src.to_str().unwrap(),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-c:s",
            "srt",
            mkv_file.to_str().unwrap(),
        ])
        .status()
        .await
        .unwrap();
    assert!(mux_status.success());

    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'movie', 'SubMovie', 'submovie', 1, 1) RETURNING id",
    )
    .bind(seeded.lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let version_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container) VALUES (?, 'mkv') RETURNING id",
    )
    .bind(item_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let part_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, probe, chapters, part_index)
         VALUES (?, ?, 1024, 1, 4000, '{}', '[]', 0) RETURNING id",
    )
    .bind(version_id)
    .bind(mkv_file.to_str().unwrap())
    .fetch_one(&db.write)
    .await
    .unwrap();

    // Stream 1: embedded SRT inside Movie.mkv
    // Stream 2: external sidecar SRT (Movie.en.srt)
    // Stream 3: external sidecar ASS (Movie.jpn.ass)
    for (idx, codec, ext_path) in [
        (1i64, "srt", None),
        (2i64, "srt", Some(sidecar_srt.to_str().unwrap())),
        (3i64, "ass", Some(sidecar_ass.to_str().unwrap())),
    ] {
        sqlx::query(
            "INSERT INTO streams (part_id, idx, kind, codec, language, is_default, is_forced, external_path)
             VALUES (?, ?, 'subtitle', ?, 'en', 0, 0, ?)",
        )
        .bind(part_id)
        .bind(idx)
        .bind(codec)
        .bind(ext_path)
        .execute(&db.write)
        .await
        .unwrap();
    }

    // 1. Convert embedded SRT (stream 1) to WebVTT
    let (status, headers, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/subtitles/{part_id}/1.vtt"),
        Some(&admin),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "text/vtt; charset=utf-8");
    let vtt_1 = String::from_utf8_lossy(&bytes);
    assert!(vtt_1.starts_with("WEBVTT"), "{vtt_1}");
    assert!(vtt_1.contains("Embedded subtitle line"), "{vtt_1}");

    // Verify cached file on disk exists and second request hits cache
    let cached_file = app.dir.path().join(format!("data/cache/subtitles/{part_id}_1.vtt"));
    assert!(cached_file.exists());
    let (status2, _, bytes2) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/subtitles/{part_id}/1.vtt"),
        Some(&admin),
        None,
        None,
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(bytes2, bytes);

    // 2. Convert external sidecar SRT (stream 2) to WebVTT
    let (status, _, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/subtitles/{part_id}/2.vtt"),
        Some(&admin),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let vtt_2 = String::from_utf8_lossy(&bytes);
    assert!(vtt_2.starts_with("WEBVTT"), "{vtt_2}");
    assert!(vtt_2.contains("Sidecar SRT line"), "{vtt_2}");

    // 3. Convert external sidecar ASS (stream 3) to WebVTT AND serve raw .ass
    let (status, _, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/subtitles/{part_id}/3.vtt"),
        Some(&admin),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let vtt_3 = String::from_utf8_lossy(&bytes);
    assert!(vtt_3.starts_with("WEBVTT"), "{vtt_3}");
    assert!(vtt_3.contains("Raw ASS dialogue"), "{vtt_3}");

    let (status, headers, bytes) = raw_request(
        &app,
        Method::GET,
        &format!("/api/v1/stream/subtitles/{part_id}/3.ass"),
        Some(&admin),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "text/x-ssa; charset=utf-8");
    assert_eq!(String::from_utf8_lossy(&bytes), raw_ass);
}
