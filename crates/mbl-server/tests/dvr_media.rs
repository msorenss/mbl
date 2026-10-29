#![allow(clippy::unwrap_used)]
mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_core::{RecordingId, StreamKind, time::now_ms};
use mbl_db::{Db, repo::m9};
use mbl_probe::FfTools;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

struct Source {
    url: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Source {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// Authored PGS display set: an 8x8 white square on a transparent 128x72 canvas,
// visible for three seconds. FFmpeg converts it to a real DVB bitmap stream.
fn caption() -> Vec<u8> {
    fn segment(out: &mut Vec<u8>, pts: u32, kind: u8, data: &[u8]) {
        out.extend(b"PG");
        out.extend(pts.to_be_bytes());
        out.extend(0_u32.to_be_bytes());
        out.push(kind);
        out.extend(u16::try_from(data.len()).unwrap().to_be_bytes());
        out.extend(data);
    }
    let mut out = Vec::new();
    let composition = [0, 128, 0, 72, 0x10, 0, 0, 0x80, 0, 0];
    let mut display = composition.to_vec();
    display.extend([1, 0, 0, 0, 0, 0, 20, 0, 40]);
    segment(&mut out, 0, 0x16, &display);
    segment(&mut out, 0, 0x17, &[1, 0, 0, 20, 0, 40, 0, 8, 0, 8]);
    segment(&mut out, 0, 0x14, &[0, 0, 0, 16, 128, 128, 0, 1, 235, 128, 128, 255]);
    let mut object = vec![0, 0, 0, 0xc0, 0, 0, 44, 0, 8, 0, 8];
    for _ in 0..8 {
        object.extend([0, 0x88, 1, 0, 0]);
    }
    segment(&mut out, 0, 0x15, &object);
    segment(&mut out, 0, 0x80, &[]);
    let mut clear = composition.to_vec();
    clear.push(0);
    segment(&mut out, 270000, 0x16, &clear);
    segment(&mut out, 270000, 0x80, &[]);
    out
}

enum Media {
    Dvb,
    Radio,
    Hevc,
}
async fn source(media: Media) -> Source {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.ts");
    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.args(["-nostdin", "-v", "error", "-y"]);
    if !matches!(media, Media::Radio) {
        cmd.args(["-f", "lavfi", "-i", "color=black:s=128x72:r=25:d=6"]);
    }
    cmd.args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=6"]);
    match media {
        Media::Dvb => {
            let sub = temp.path().join("caption.sup");
            tokio::fs::write(&sub, caption()).await.unwrap();
            cmd.args(["-f", "lavfi", "-i", "sine=frequency=880:sample_rate=48000:duration=6"])
                .arg("-i")
                .arg(sub)
                .args([
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-map",
                    "2:a",
                    "-map",
                    "3:s",
                    "-c:v",
                    "mpeg2video",
                    "-threads",
                    "1",
                    "-g",
                    "25",
                    "-c:a",
                    "ac3",
                    "-c:s",
                    "dvbsub",
                    "-metadata:s:a:0",
                    "language=eng",
                    "-metadata:s:a:1",
                    "language=swe",
                    "-metadata:s:s:0",
                    "language=swe",
                ]);
        }
        Media::Radio => {
            cmd.args(["-map", "0:a", "-c:a", "mp2", "-metadata:s:a:0", "language=swe"]);
        }
        Media::Hevc => {
            cmd.args([
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-c:v",
                "libx265",
                "-threads",
                "1",
                "-x265-params",
                "pools=none:frame-threads=1:log-level=error",
                "-c:a",
                "aac",
            ]);
        }
    }
    let result = cmd.args(["-f", "mpegts"]).arg(&path).output().await.unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let bytes = axum::body::Bytes::from(tokio::fs::read(path).await.unwrap());
    let router = axum::Router::new().route(
        "/source.ts",
        axum::routing::get(move || {
            let bytes = bytes.clone();
            async move { ([("Content-Type", "video/mp2t")], bytes) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/source.ts", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Source { url, task }
}

async fn capture(
    source: &Source,
    seconds: i64,
) -> (TestApp, Db, mbl_service::Services, String, m9::RecordingRow) {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let tuner = app
        .call(
            Method::POST,
            "/api/v1/livetv/tuners/m3u",
            Some(&admin),
            Some(json!({
                "name":"DVR media", "content":format!("#EXTM3U\n#EXTINF:-1,Media\n{}\n",source.url)
            })),
        )
        .await;
    assert_eq!(tuner.status, StatusCode::OK);
    let channel: i64 =
        sqlx::query_scalar("SELECT id FROM channels LIMIT 1").fetch_one(&db.read).await.unwrap();
    let now = now_ms();
    let rec = app.call(Method::POST,"/api/v1/livetv/recordings",Some(&admin),Some(json!({
        "channelId":channel,"title":"Broadcast recording","startTime":now-20,"endTime":now+seconds*1000
    }))).await;
    assert_eq!(rec.status, StatusCode::OK, "{}", rec.body);
    let id = RecordingId(rec.body["id"].as_i64().unwrap());
    sqlx::query("UPDATE recordings SET content_rating='G' WHERE id=?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    let stop = CancellationToken::new();
    let task = tokio::spawn(mbl_server::recorder::run(
        db.clone(),
        services.livetv.clone(),
        services.playback.clone(),
        services.stream.ffmpeg_bin().to_path_buf(),
        app.dir.path().join("data/recordings"),
        stop.clone(),
    ));
    let row = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let row = m9::get_recording(&db, id).await.unwrap().unwrap();
            if matches!(row.status.as_str(), "completed" | "partial" | "failed") {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    stop.cancel();
    task.await.unwrap();
    (app, db, services, admin, row.unwrap())
}
async fn playlist(services: &mbl_service::Services, response: &Value) -> PathBuf {
    let session = services.stream.hls_session(response["sessionId"].as_str().unwrap()).unwrap();
    session.request_segment("0", 0).await.unwrap();
    let path = session.dir.join("0/ffmpeg.m3u8").into_std_path_buf();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if tokio::fs::read_to_string(&path).await.is_ok_and(|p| p.contains("#EXT-X-ENDLIST")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    path
}
async fn stop_playback(app: &TestApp, token: &str, response: &Value) {
    assert_eq!(
        app.call(
            Method::POST,
            &format!("/api/v1/playback/{}/stop", response["sessionId"].as_str().unwrap()),
            Some(token),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn broadcast_audio_languages_and_dvb_pixels_survive_capture_and_playback() {
    let source = source(Media::Dvb).await;
    let (app, db, services, admin, row) = capture(&source, 120).await;
    assert_eq!(row.status, "partial", "{:?}", row.error_message);
    let tools = FfTools::locate("", "").await.unwrap();
    let path = PathBuf::from(row.output_path.unwrap());
    let info = tools.probe(&path, Duration::from_secs(10)).await.unwrap();
    assert_eq!(info.primary_video().unwrap().codec, "mpeg2video");
    let audio: Vec<_> = info.streams_of(StreamKind::Audio).collect();
    assert_eq!(audio.len(), 2);
    assert_eq!(audio[0].language.as_deref(), Some("eng"));
    assert_eq!(audio[1].language.as_deref(), Some("swe"));
    let sub = info.streams_of(StreamKind::Subtitle).next().unwrap();
    assert_eq!(sub.codec, "dvb_subtitle");
    assert_eq!(sub.language.as_deref(), Some("swe"));
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM streams WHERE part_id=(SELECT id FROM media_parts WHERE version_id=(SELECT id FROM media_versions WHERE item_id=?)) AND kind='subtitle'").bind(row.item_id).fetch_one(&db.read).await.unwrap();
    assert_eq!(stored, 1);
    let play = app.call(Method::POST,"/api/v1/playback",Some(&admin),Some(json!({
        "itemId":row.item_id,"profile":"chrome","waitForReady":true,"audioStreamIndex":audio[1].index,"subtitleStreamIndex":sub.index
    }))).await;
    assert_eq!(play.status, StatusCode::OK, "{}", play.body);
    assert_eq!(play.body["hasVideo"], true);
    assert_eq!(play.body["audioStreamIndex"], audio[1].index);
    assert_eq!(play.body["audioTracks"][1]["language"], "swe");
    assert_eq!(play.body["burnedSubtitleStreamIndex"], sub.index);
    assert_eq!(play.body["subtitleOptions"][0]["requiresTranscode"], true);
    let hls = playlist(&services, &play.body).await;
    let picture = tokio::process::Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&hls)
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
    assert!(picture.stdout.iter().any(|p| *p > 180), "DVB caption must appear on black video");
    let output = tools.probe(&hls, Duration::from_secs(10)).await.unwrap();
    assert_eq!(output.primary_video().unwrap().codec, "h264");
    assert_eq!(output.streams_of(StreamKind::Audio).next().unwrap().codec, "aac");
    stop_playback(&app, &admin, &play.body).await;
    let off = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":row.item_id,"profile":"chrome","waitForReady":true})),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    assert!(off.body["burnedSubtitleStreamIndex"].is_null());
    let hls = playlist(&services, &off.body).await;
    let picture = tokio::process::Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
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
    assert!(picture.status.success());
    assert!(picture.stdout.iter().all(|p| *p < 100), "Off must remove the DVB caption");
    stop_playback(&app, &admin, &off.body).await;
}

#[tokio::test]
async fn radio_is_a_playable_track_with_history_profile_guards_and_owned_deletion() {
    let source = source(Media::Radio).await;
    for (seconds, status) in [(4, "completed"), (120, "partial")] {
        let (app, db, services, admin, row) = capture(&source, seconds).await;
        assert_eq!(row.status, status, "{:?}", row.error_message);
        assert!(row.duration_ms.unwrap() > 3000);
        let item = row.item_id.unwrap();
        let catalogue =
            app.call(Method::GET, &format!("/api/v1/items/{item}"), Some(&admin), None).await;
        assert_eq!(catalogue.body["kind"], "track");
        let viewer = app.create_user(&admin, "radio-viewer").await;
        let user = services.auth.authenticate(&viewer).await.unwrap().actor.user_id;
        sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling='G' WHERE id=?")
            .bind(user.0)
            .execute(&db.write)
            .await
            .unwrap();
        mbl_db::repo::m6::grant_library_access(
            &db,
            user,
            mbl_core::LibraryId(row.library_id.unwrap()),
        )
        .await
        .unwrap();
        let play = app
            .call(
                Method::POST,
                "/api/v1/playback",
                Some(&viewer),
                Some(json!({"itemId":item,"profile":"chrome","waitForReady":true})),
            )
            .await;
        assert_eq!(play.status, StatusCode::OK, "{}", play.body);
        assert_eq!(play.body["hasVideo"], false);
        assert_eq!(play.body["audioTracks"][0]["codec"], "mp2");
        let master = app.call(Method::GET, play.body["url"].as_str().unwrap(), None, None).await;
        assert_eq!(master.status, StatusCode::OK);
        let master = master.body.as_str().unwrap();
        assert!(master.contains("CODECS=\"mp4a.40.2\""), "{master}");
        assert!(!master.contains("RESOLUTION="));
        assert!(!master.contains("avc1"));
        assert_eq!(services.stream.hls_manager().unwrap().active_video_transcode_count(), 0);
        let hls = playlist(&services, &play.body).await;
        let tools = FfTools::locate("", "").await.unwrap();
        let output = tools.probe(&hls, Duration::from_secs(10)).await.unwrap();
        assert!(output.primary_video().is_none());
        assert_eq!(output.streams_of(StreamKind::Audio).next().unwrap().codec, "aac");
        assert_eq!(
            app.call(
                Method::POST,
                &format!("/api/v1/playback/{}/progress", play.body["sessionId"].as_str().unwrap()),
                Some(&viewer),
                Some(json!({"positionMs":1000,"state":"playing"}))
            )
            .await
            .status,
            StatusCode::NO_CONTENT
        );
        stop_playback(&app, &viewer, &play.body).await;
        let history: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM watch_history WHERE user_id=? AND item_id=?")
                .bind(user.0)
                .bind(item)
                .fetch_one(&db.read)
                .await
                .unwrap();
        assert_eq!(history, 1);
        assert_eq!(
            app.call(
                Method::DELETE,
                &format!("/api/v1/livetv/recordings/{}", row.id),
                Some(&viewer),
                None
            )
            .await
            .status,
            StatusCode::FORBIDDEN
        );
        mbl_db::repo::m6::set_user_libraries(&db, user, &[]).await.unwrap();
        assert_eq!(
            app.call(
                Method::POST,
                "/api/v1/playback",
                Some(&viewer),
                Some(json!({"itemId":item,"profile":"chrome"}))
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        let path = PathBuf::from(row.output_path.unwrap());
        assert_eq!(
            app.call(
                Method::DELETE,
                &format!("/api/v1/livetv/recordings/{}", row.id),
                Some(&admin),
                None
            )
            .await
            .status,
            StatusCode::OK
        );
        assert!(!path.exists());
    }
}

#[tokio::test]
async fn hevc_and_aac_are_preserved_and_negotiated_for_a_browser() {
    let source = source(Media::Hevc).await;
    let (app, _db, services, admin, row) = capture(&source, 120).await;
    assert_eq!(row.status, "partial", "{:?}", row.error_message);
    let tools = FfTools::locate("", "").await.unwrap();
    let info = tools
        .probe(&PathBuf::from(row.output_path.unwrap()), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(info.primary_video().unwrap().codec, "hevc");
    assert_eq!(info.streams_of(StreamKind::Audio).next().unwrap().codec, "aac");
    let play = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":row.item_id,"profile":"firefox","waitForReady":true})),
        )
        .await;
    assert_eq!(play.status, StatusCode::OK, "{}", play.body);
    assert_eq!(play.body["hasVideo"], true);
    let hls = playlist(&services, &play.body).await;
    let output = tools.probe(&hls, Duration::from_secs(10)).await.unwrap();
    assert_eq!(output.primary_video().unwrap().codec, "h264");
    stop_playback(&app, &admin, &play.body).await;
    let copy = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":row.item_id,"profile":"safari","waitForReady":true})),
        )
        .await;
    assert_eq!(copy.status, StatusCode::OK, "{}", copy.body);
    let master = app.call(Method::GET, copy.body["url"].as_str().unwrap(), None, None).await;
    assert_eq!(master.status, StatusCode::OK);
    let master = master.body.as_str().unwrap();
    assert!(master.contains("hvc1"), "{master}");
    assert!(!master.contains("avc1"));
    let hls = playlist(&services, &copy.body).await;
    let output = tools.probe(&hls, Duration::from_secs(10)).await.unwrap();
    assert_eq!(output.primary_video().unwrap().codec, "hevc");
    let tags = tokio::process::Command::new(&tools.ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_tag_string",
            "-of",
            "json",
        ])
        .arg(&hls)
        .output()
        .await
        .unwrap();
    assert!(tags.status.success());
    let tags: Value = serde_json::from_slice(&tags.stdout).unwrap();
    assert_eq!(tags["streams"][0]["codec_tag_string"], "hvc1");
    stop_playback(&app, &admin, &copy.body).await;
}

#[tokio::test]
async fn subtitle_only_files_with_a_real_duration_are_not_published_as_radio() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let imported = app.call(Method::POST,"/api/v1/livetv/tuners/m3u",Some(&admin),Some(json!({
        "name":"Caption only", "content":"#EXTM3U\n#EXTINF:-1,Caption only\nhttp://127.0.0.1:1/unused\n"
    }))).await;
    assert_eq!(imported.status, StatusCode::OK);
    let channel: i64 =
        sqlx::query_scalar("SELECT id FROM channels LIMIT 1").fetch_one(&db.read).await.unwrap();
    let now = now_ms();
    let rec = app
        .call(
            Method::POST,
            "/api/v1/livetv/recordings",
            Some(&admin),
            Some(json!({
                "channelId":channel,"title":"Caption only","startTime":now,"endTime":now+120000
            })),
        )
        .await;
    assert_eq!(rec.status, StatusCode::OK, "{}", rec.body);
    let id = RecordingId(rec.body["id"].as_i64().unwrap());
    let path = app.dir.path().join(format!("data/recordings/{}.ts", id.0));
    let caption = app.dir.path().join("caption.srt");
    tokio::fs::write(&caption, b"1\n00:00:00,000 --> 00:00:03,000\nOnly a caption\n")
        .await
        .unwrap();
    let tools = FfTools::locate("", "").await.unwrap();
    // Simulate an encoder producing readable, positive-duration metadata without A/V.
    let result = tokio::process::Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(caption)
        .args(["-map", "0:s", "-c:s", "copy", "-f", "matroska"])
        .arg(&path)
        .output()
        .await
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let info = tools.probe(&path, Duration::from_secs(10)).await.unwrap();
    assert!(info.duration_ms > 0);
    assert_eq!(info.streams_of(StreamKind::Subtitle).count(), 1);
    m9::update_recording_status(
        &db,
        id,
        "recording",
        Some(path.to_str().unwrap()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let error = services.livetv.publish_recording_capture(id, None).await.unwrap_err();
    assert!(error.to_string().contains("no playable audio/video"));
    assert!(m9::get_recording(&db, id).await.unwrap().unwrap().item_id.is_none());
}
