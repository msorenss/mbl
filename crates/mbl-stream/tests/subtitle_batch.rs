//! One FFmpeg run takes every text subtitle track out of a file (real FFmpeg).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use mbl_core::PartId;
use mbl_stream::subtitles::{cached_vtt_path, extract_embedded_vtt};

fn srt(dir: &Path, name: &str, line: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("1\n00:00:00,500 --> 00:00:01,500\n{line}\n")).unwrap();
    path
}

#[tokio::test]
async fn all_text_tracks_come_out_in_one_run() {
    let dir = tempfile::tempdir().unwrap();
    let (en, sv, de) = (
        srt(dir.path(), "en.srt", "Hello"),
        srt(dir.path(), "sv.srt", "Hej"),
        srt(dir.path(), "de.srt", "Hallo"),
    );
    let media = dir.path().join("film.mkv");
    let status = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=black:s=64x36:d=2"])
        .args(["-i"])
        .arg(&en)
        .args(["-i"])
        .arg(&sv)
        .args(["-i"])
        .arg(&de)
        .args([
            "-map", "0", "-map", "1", "-map", "2", "-map", "3", "-c:v", "libx264", "-c:s", "srt",
        ])
        .arg(&media)
        .status()
        .expect("ffmpeg");
    assert!(status.success());

    let cache = dir.path().join("cache");
    let part = PartId(7);
    let ffmpeg = Path::new("ffmpeg");
    // Streams 1–3 are the subtitles; stream 2 is already cached and must be left alone.
    std::fs::create_dir_all(cache.join("subtitles")).unwrap();
    std::fs::write(cached_vtt_path(&cache, part, 2), "WEBVTT\n\nkept").unwrap();

    let written =
        extract_embedded_vtt(&cache, ffmpeg, part, &media, &[1, 2, 3], None).await.unwrap();
    assert_eq!(written, 2);
    let read = |i| std::fs::read_to_string(cached_vtt_path(&cache, part, i)).unwrap();
    assert!(read(1).contains("Hello"), "{}", read(1));
    assert_eq!(read(2), "WEBVTT\n\nkept");
    assert!(read(3).contains("Hallo"), "{}", read(3));
    // Nothing left to do the second time; no temporary files remain.
    assert_eq!(
        extract_embedded_vtt(&cache, ffmpeg, part, &media, &[1, 2, 3], None).await.unwrap(),
        0
    );
    let leftovers = std::fs::read_dir(cache.join("subtitles"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "tmp"))
        .count();
    assert_eq!(leftovers, 0);

    // A track FFmpeg cannot turn into WebVTT (here: a video stream) fails the run and writes nothing.
    let other = PartId(8);
    assert!(extract_embedded_vtt(&cache, ffmpeg, other, &media, &[0, 1], None).await.is_err());
    assert!(!cached_vtt_path(&cache, other, 1).exists());
}

#[tokio::test]
async fn the_same_run_hands_over_the_audio_for_speech_detection() {
    let dir = tempfile::tempdir().unwrap();
    let en = srt(dir.path(), "en.srt", "Hello");
    let media = dir.path().join("film.mkv");
    let status = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=black:s=64x36:d=3"])
        .args(["-f", "lavfi", "-i", "sine=f=440:d=3", "-i"])
        .arg(&en)
        .args(["-map", "0", "-map", "1", "-map", "2", "-c:v", "libx264", "-c:a", "aac"])
        .args(["-c:s", "srt"])
        .arg(&media)
        .status()
        .expect("ffmpeg");
    assert!(status.success());
    let cache = dir.path().join("cache");
    let mut bytes = 0usize;
    let mut feed = |chunk: &[u8]| bytes += chunk.len();
    let written =
        extract_embedded_vtt(&cache, Path::new("ffmpeg"), PartId(9), &media, &[2], Some(&mut feed))
            .await
            .unwrap();
    assert_eq!(written, 1);
    // About three seconds of 8 kHz 16-bit mono.
    assert!((44_000..=50_000).contains(&bytes), "{bytes} bytes of audio");
    assert!(
        std::fs::read_to_string(cached_vtt_path(&cache, PartId(9), 2)).unwrap().contains("Hello")
    );
}
