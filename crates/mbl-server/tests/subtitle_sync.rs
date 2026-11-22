//! Subtitles matched to the speech: a real film whose subtitles are two seconds late.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;

/// Cue times (start, end) in a WebVTT body, in milliseconds.
fn cue_starts(vtt: &str) -> Vec<i64> {
    vtt.lines()
        .filter_map(|l| l.split_once("-->").map(|(a, _)| a.trim().to_owned()))
        .map(|t| {
            let (clock, ms) = t.split_once('.').unwrap();
            let parts: Vec<i64> = clock.split(':').map(|p| p.parse().unwrap()).collect();
            let secs = parts.iter().fold(0, |acc, p| acc * 60 + p);
            secs * 1000 + ms.parse::<i64>().unwrap()
        })
        .collect()
}

#[tokio::test]
async fn late_subtitles_are_moved_to_the_speech_when_asked() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;

    // A minute of film where "someone speaks" from 2 to 4 s of every 6 s.
    let movies = std::path::Path::new(&app.media_dir("Movies")).to_owned();
    let movie = movies.join("Heat (1995).mp4");
    let made = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i"])
        .arg("color=c=black:s=64x36:d=60")
        .args(["-f", "lavfi", "-i"])
        .arg("aevalsrc='if(between(mod(t,6),2,4),0.5*sin(2*PI*600*t),0.002*sin(2*PI*90*t))':s=16000:d=60")
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac"])
        .arg(&movie)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());
    // Subtitles timed two seconds late, as a file made for another release often is.
    let srt: String = (0..10)
        .map(|i| {
            let start = i * 6 + 4;
            format!("{}\n00:00:{start:02},000 --> 00:00:{:02},000\nLine {i}\n\n", i + 1, start + 2)
        })
        .collect();
    let srt_path = movies.join("Heat (1995).en.srt");
    std::fs::write(&srt_path, srt).unwrap();
    for (idx, kind, codec, external) in [
        (1, "audio", "aac", None),
        (2, "subtitle", "subrip", Some(srt_path.to_str().unwrap().to_owned())),
    ] {
        sqlx::query(
            "INSERT INTO streams (part_id, idx, kind, codec, language, is_default, external_path)
             VALUES (?, ?, ?, ?, 'eng', 1, ?)",
        )
        .bind(fixture.movie_part_id)
        .bind(idx)
        .bind(kind)
        .bind(codec)
        .bind(external)
        .execute(&db.write)
        .await
        .unwrap();
    }

    let base = format!("/api/v1/stream/subtitles/{}/2.vtt", fixture.movie_part_id);
    let text = |res: &common::Res| res.body.as_str().unwrap_or_default().to_owned();
    let plain = app.call(Method::GET, &base, Some(&admin), None).await;
    assert_eq!(plain.status, StatusCode::OK, "{}", plain.body);
    assert_eq!(cue_starts(&text(&plain))[..3], [4_000, 10_000, 16_000]);

    let synced = app.call(Method::GET, &format!("{base}?sync=true"), Some(&admin), None).await;
    assert_eq!(synced.status, StatusCode::OK, "{}", synced.body);
    let synced = text(&synced);
    let starts = cue_starts(&synced);
    assert_eq!(starts.len(), 10);
    for (i, start) in starts.iter().enumerate() {
        let expected = 2_000 + i as i64 * 6_000;
        assert!((start - expected).abs() <= 200, "cue {i} at {start} ms, expected ~{expected}");
    }
    assert!(synced.contains("Line 9"));
}
