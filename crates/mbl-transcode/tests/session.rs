#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use camino::Utf8PathBuf;
use mbl_core::{Error, PartId, UserId};
use mbl_platform::PlatformProfile;
use mbl_transcode::{
    AudioCodec, AudioOp, Backend, CreateSessionReq, FfmpegCaps, HlsOutput, Pipeline, Rendition,
    SessionManager, SessionManagerConfig, SubtitleOp, VideoCodec, VideoOp,
};

async fn generate_synthetic_media(ffmpeg: &Path, out_path: &Path, duration_secs: u32) {
    let status = tokio::process::Command::new(ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=duration={duration_secs}:size=320x180:rate=24"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:sample_rate=48000:duration={duration_secs}"),
            "-c:v",
            "mpeg4",
            "-q:v",
            "5",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            out_path.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .unwrap();
    assert!(status.success(), "failed to generate synthetic test media");
}

fn sample_pipeline(input: &Path, segment_secs: u32, video: VideoOp) -> Pipeline {
    Pipeline {
        input: Utf8PathBuf::from_path_buf(input.to_path_buf()).unwrap(),
        start_ms: 0,
        video,
        audio: AudioOp::Encode { index: 1, codec: AudioCodec::Aac, channels: 2, bitrate: 128_000 },
        subtitle: SubtitleOp::None,
        output: HlsOutput {
            dir: Utf8PathBuf::from("/tmp/placeholder"),
            segment_secs,
            start_number: 0,
            renditions: vec![Rendition {
                name: "0".to_owned(),
                width: Some(320),
                max_bitrate: 600_000,
            }],
        },
        backend: Backend::Software,
    }
}

#[tokio::test]
async fn seek_far_restarts_ffmpeg_and_serves_target_segment() {
    let Ok(tools) = mbl_probe::FfTools::locate("", "").await else {
        eprintln!("skipping: ffmpeg not available");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let media_file = tmp.path().join("movie_30s.mkv");
    generate_synthetic_media(&tools.ffmpeg, &media_file, 30).await;

    let transcode_dir = Utf8PathBuf::from_path_buf(tmp.path().join("transcode")).unwrap();
    let mut cfg = SessionManagerConfig::new(tools.ffmpeg, transcode_dir, PlatformProfile::Generic);
    cfg.caps = FfmpegCaps { preset: "ultrafast".to_owned(), crf: 28, threads: Some(2) };
    let mgr = SessionManager::new(cfg).unwrap();

    let session = mgr
        .create_session(CreateSessionReq {
            user_id: UserId(1),
            part_id: PartId(10),
            duration_ms: 30_000,
            pipeline: sample_pipeline(
                &media_file,
                2,
                VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(320),
                    max_bitrate: 600_000,
                    deinterlace: false,
                    tonemap: false,
                },
            ),
            master_playlist: "#EXTM3U\n0/index.m3u8\n".to_owned(),
            media_playlists: vec![("0".to_owned(), "#EXTM3U\n".to_owned())],
            fast_start_segments: false,
        })
        .await
        .unwrap();

    // Upfront playlists exist immediately.
    assert_eq!(session.request_master_playlist().await.unwrap(), "#EXTM3U\n0/index.m3u8\n");
    assert_eq!(session.request_media_playlist("0").await.unwrap(), "#EXTM3U\n");

    let init = session.request_init("0").await.unwrap();
    assert!(init.exists());
    let seg0 = session.request_segment("0", 0).await.unwrap();
    assert!(seg0.exists());
    assert_eq!(session.restart_count().await, 0);

    // Seek far ahead to segment 12 (24s into the 30s file, > head + 3): must restart FFmpeg.
    let seg12 = session.request_segment("0", 12).await.unwrap();
    assert!(seg12.exists());
    assert!(std::fs::metadata(&seg12).unwrap().len() > 0, "12.m4s must be non-empty");
    assert!(
        session.restart_count().await >= 1,
        "seeking far ahead must restart FFmpeg at segment 12"
    );

    mgr.shutdown_all().await;
}

#[tokio::test]
async fn ahead_throttling_pauses_and_resumes_ffmpeg() {
    let Ok(tools) = mbl_probe::FfTools::locate("", "").await else {
        eprintln!("skipping: ffmpeg not available");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    // 36-second input with 1-second segments so FFmpeg quickly gets >2 segments ahead of segment 0
    // while still having plenty of work remaining.
    let media_file = tmp.path().join("long_36s.mkv");
    generate_synthetic_media(&tools.ffmpeg, &media_file, 36).await;

    let transcode_dir = Utf8PathBuf::from_path_buf(tmp.path().join("transcode")).unwrap();
    let mut cfg = SessionManagerConfig::new(tools.ffmpeg, transcode_dir, PlatformProfile::Generic);
    cfg.ahead_segments = 2;
    cfg.caps = FfmpegCaps { preset: "medium".to_owned(), crf: 23, threads: Some(1) };
    let mgr = SessionManager::new(cfg).unwrap();

    let session = mgr
        .create_session(CreateSessionReq {
            user_id: UserId(1),
            part_id: PartId(20),
            duration_ms: 36_000,
            pipeline: sample_pipeline(
                &media_file,
                1,
                VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(320),
                    max_bitrate: 600_000,
                    deinterlace: false,
                    tonemap: false,
                },
            ),
            master_playlist: "#EXTM3U\n".to_owned(),
            media_playlists: vec![("0".to_owned(), "#EXTM3U\n".to_owned())],
            fast_start_segments: false,
        })
        .await
        .unwrap();

    // Request segment 0, then wait until segment 3 is completed on disk while client stays at 0.
    let _ = session.request_segment("0", 0).await.unwrap();
    let seg4_file = session.dir.join("0/4.m4s");
    for _ in 0..200 {
        if seg4_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    session.tick_throttle().await;
    if session.ffmpeg_pid().await.is_some() {
        assert!(
            session.is_paused().await,
            "FFmpeg must be paused once it is more than ahead_segments=2 ahead of segment 0"
        );

        #[cfg(target_os = "linux")]
        if let Some(pid) = session.ffmpeg_pid().await {
            let mut state_char = '?';
            for _ in 0..25 {
                if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                    && let Some(after_comm) = stat.rsplit_once(") ").map(|(_, rest)| rest)
                {
                    state_char = after_comm.chars().next().unwrap_or('?');
                    if state_char == 'T' {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert_eq!(state_char, 'T', "Linux process state must be T (stopped)");
        }

        // Now catch up by requesting segment 3 -> must resume FFmpeg.
        let _ = session.request_segment("0", 3).await.unwrap();
        assert!(
            !session.is_paused().await,
            "FFmpeg must resume once client catches up within ahead_segments"
        );
    }

    mgr.shutdown_all().await;
}

#[tokio::test]
async fn inactivity_timeout_kills_ffmpeg_and_deletes_session_dir() {
    let Ok(tools) = mbl_probe::FfTools::locate("", "").await else {
        eprintln!("skipping: ffmpeg not available");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let media_file = tmp.path().join("short_6s.mkv");
    generate_synthetic_media(&tools.ffmpeg, &media_file, 6).await;

    let transcode_dir = Utf8PathBuf::from_path_buf(tmp.path().join("transcode")).unwrap();
    let mut cfg = SessionManagerConfig::new(tools.ffmpeg, transcode_dir, PlatformProfile::Pi5);
    cfg.inactivity_timeout = Duration::from_millis(120);
    cfg.caps = FfmpegCaps { preset: "ultrafast".to_owned(), crf: 28, threads: Some(1) };
    let mgr = SessionManager::new(cfg).unwrap();

    let session = mgr
        .create_session(CreateSessionReq {
            user_id: UserId(1),
            part_id: PartId(30),
            duration_ms: 6_000,
            pipeline: sample_pipeline(&media_file, 2, VideoOp::Copy),
            master_playlist: "#EXTM3U\n".to_owned(),
            media_playlists: vec![("0".to_owned(), "#EXTM3U\n".to_owned())],
            fast_start_segments: false,
        })
        .await
        .unwrap();

    let session_id = session.id.clone();
    let session_dir = session.dir.clone();
    assert!(session_dir.exists());

    tokio::time::sleep(Duration::from_millis(180)).await;
    mgr.run_maintenance_once().await;

    assert!(mgr.get_session(&session_id).is_none(), "expired session must be removed");
    assert!(!session_dir.exists(), "expired session directory must be deleted");
}

#[tokio::test]
async fn concurrency_cap_and_disk_quota_enforcement() {
    let Ok(tools) = mbl_probe::FfTools::locate("", "").await else {
        eprintln!("skipping: ffmpeg not available");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let media_file = tmp.path().join("cap_6s.mkv");
    generate_synthetic_media(&tools.ffmpeg, &media_file, 6).await;

    let transcode_dir = Utf8PathBuf::from_path_buf(tmp.path().join("transcode")).unwrap();
    // Pi 5 profile defaults to max_video_sessions = 1, max_remux_sessions = 8.
    let mut cfg =
        SessionManagerConfig::new(tools.ffmpeg, transcode_dir.clone(), PlatformProfile::Pi5);
    cfg.caps = FfmpegCaps { preset: "ultrafast".to_owned(), crf: 28, threads: Some(1) };
    // Tiny quota (1 KiB) to exercise quota enforcement.
    cfg.temp_quota_bytes = 1024;
    let mgr = SessionManager::new(cfg).unwrap();

    assert!(mgr.has_video_transcode_capacity());

    let s1 = mgr
        .create_session(CreateSessionReq {
            user_id: UserId(1),
            part_id: PartId(40),
            duration_ms: 6_000,
            pipeline: sample_pipeline(
                &media_file,
                2,
                VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(320),
                    max_bitrate: 500_000,
                    deinterlace: false,
                    tonemap: false,
                },
            ),
            master_playlist: "#EXTM3U\n".to_owned(),
            media_playlists: vec![("0".to_owned(), "#EXTM3U\n".to_owned())],
            fast_start_segments: false,
        })
        .await
        .unwrap();

    assert!(!mgr.has_video_transcode_capacity());

    // Second concurrent video transcode on Pi 5 (max_video_sessions = 1) must be rejected.
    let second_video = mgr
        .create_session(CreateSessionReq {
            user_id: UserId(1),
            part_id: PartId(41),
            duration_ms: 6_000,
            pipeline: sample_pipeline(
                &media_file,
                2,
                VideoOp::Encode {
                    codec: VideoCodec::H264,
                    width: Some(320),
                    max_bitrate: 500_000,
                    deinterlace: false,
                    tonemap: false,
                },
            ),
            master_playlist: "#EXTM3U\n".to_owned(),
            media_playlists: vec![("0".to_owned(), "#EXTM3U\n".to_owned())],
            fast_start_segments: false,
        })
        .await;
    assert!(matches!(second_video, Err(Error::Unavailable(_))));

    // Remux session (`VideoOp::Copy`) is still allowed alongside the video transcode!
    let remux = mgr
        .create_session(CreateSessionReq {
            user_id: UserId(1),
            part_id: PartId(42),
            duration_ms: 6_000,
            pipeline: sample_pipeline(&media_file, 2, VideoOp::Copy),
            master_playlist: "#EXTM3U\n".to_owned(),
            media_playlists: vec![("0".to_owned(), "#EXTM3U\n".to_owned())],
            fast_start_segments: false,
        })
        .await
        .unwrap();

    // Wait for s1 to produce segment 0 so disk usage exceeds the 1 KiB quota.
    let _ = s1.request_segment("0", 0).await.unwrap();
    mgr.enforce_disk_quota().await;

    // Quota enforcement must have evicted at least the oldest session (`s1`).
    assert!(mgr.get_session(&s1.id).is_none(), "oldest session must be evicted when over quota");
    let _ = mgr.stop_session(&remux.id).await;
}
