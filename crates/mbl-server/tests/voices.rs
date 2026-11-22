//! Reading aloud on the server: install Piper and a voice from a (fake) download server, then
//! turn text into a WAV file with a stand-in for Piper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_core::{Actor, UserId};
use serde_json::json;

/// A stand-in for Piper: reads the text on stdin and writes a small WAV to `--output_file`.
const FAKE_PIPER: &str = r#"#!/bin/sh
out=""
while [ $# -gt 0 ]; do
  if [ "$1" = "--output_file" ]; then out="$2"; shift; fi
  shift
done
text=$(cat)
[ -n "$text" ] || exit 1
printf 'RIFF\044\000\000\000WAVEfmt \020\000\000\000\001\000\001\000\042\126\000\000\104\254\000\000\002\000\020\000data\000\000\000\000' > "$out"
"#;

#[tokio::test]
async fn piper_and_a_voice_are_installed_and_read_text_aloud() {
    let (app, _db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;

    // Nothing installed: the server cannot read aloud yet.
    let res = app.call(Method::GET, "/api/v1/speech/status", Some(&admin), None).await;
    assert_eq!(res.body["serverVoices"], json!([]), "{}", res.body);
    let res = app
        .call(
            Method::POST,
            "/api/v1/speech/say",
            Some(&admin),
            Some(json!({"text": "Hej", "language": "sv"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE, "{}", res.body);

    // A download server with a Piper archive (a `piper/` folder) and the Swedish voice.
    let staging = tempfile::tempdir().unwrap();
    let folder = staging.path().join("piper");
    std::fs::create_dir_all(&folder).unwrap();
    let program = folder.join("piper");
    std::fs::write(&program, FAKE_PIPER).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let archive = staging.path().join("piper.tar.gz");
    assert!(
        std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(staging.path())
            .arg("piper")
            .status()
            .unwrap()
            .success()
    );
    let archive_bytes = std::fs::read(&archive).unwrap();
    let router = axum::Router::new()
        .route(
            "/{archive}",
            axum::routing::get(move || {
                let bytes = archive_bytes.clone();
                async move { bytes }
            }),
        )
        .route(
            "/sv/sv_SE/nst/medium/{file}",
            axum::routing::get(
                |axum::extract::Path(file): axum::extract::Path<String>| async move {
                    if file.ends_with(".json") { "{}".as_bytes().to_vec() } else { vec![7u8; 4096] }
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let voices = services.voices.clone().with_test_sources(base.clone(), base, None);
    let actor = Actor::admin(UserId(1));
    let wait = || async {
        for _ in 0..100 {
            let status = voices.status(&actor).await.unwrap();
            if status.download.as_ref().is_some_and(|d| d.finished) {
                return status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("the download never finished");
    };
    // The fake Piper is a shell script, so the install-and-speak part runs on Unix only.
    if cfg!(unix) && voices.status(&actor).await.unwrap().supported {
        voices.install_program(&actor).await.unwrap();
        let status = wait().await;
        assert_eq!(status.download.clone().unwrap().error, None);
        assert!(status.program_installed, "{status:?}");
        voices.install_voice(&actor, "sv").await.unwrap();
        let status = wait().await;
        assert_eq!(status.download.unwrap().error, None);
        assert!(status.voices.iter().any(|v| v.language == "sv" && v.installed));
        assert!(voices.install_voice(&actor, "xx").await.is_err());

        // The server's own service sees the same files.
        let res = app.call(Method::GET, "/api/v1/speech/status", Some(&admin), None).await;
        assert_eq!(res.body["serverVoices"], json!(["sv"]), "{}", res.body);
        let res = app
            .call(
                Method::POST,
                "/api/v1/speech/say",
                Some(&admin),
                Some(json!({"text": "Hej! Vem är det där?", "language": "sv"})),
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "{}", res.body);
        assert_eq!(res.headers.get("content-type").unwrap(), "audio/wav");
        // No voice for German: the browser is told so.
        let res = app
            .call(
                Method::POST,
                "/api/v1/speech/say",
                Some(&admin),
                Some(json!({"text": "Hallo", "language": "de"})),
            )
            .await;
        assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);

        let res = app.call(Method::DELETE, "/api/v1/speech/voices/sv", Some(&admin), None).await;
        assert_eq!(res.status, StatusCode::NO_CONTENT);
        let res = app.call(Method::GET, "/api/v1/speech/status", Some(&admin), None).await;
        assert_eq!(res.body["serverVoices"], json!([]));
    }

    // Only administrators manage voices.
    let viewer = app.create_user(&admin, "viewer").await;
    let res = app.call(Method::GET, "/api/v1/speech/voices", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let res = app.call(Method::POST, "/api/v1/speech/piper", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}
