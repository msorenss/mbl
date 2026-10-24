#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::TestApp;
use futures_util::StreamExt;
use mbl_server::Config;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_util::sync::CancellationToken;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(addr: SocketAddr, token: &str) -> Ws {
    let mut req = format!("ws://{addr}/api/v1/events").into_client_request().unwrap();
    req.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
    tokio_tungstenite::connect_async(req).await.unwrap().0
}

/// Read JSON frames until `stop` matches one; returns everything seen.
async fn collect_until(ws: &mut Ws, stop: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut seen = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(msg) = ws.next().await {
            if let Message::Text(text) = msg.unwrap() {
                let v: Value = serde_json::from_str(&text).unwrap();
                let done = stop(&v);
                seen.push(v);
                if done {
                    break;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for events");
    seen
}

#[tokio::test]
async fn scan_progress_is_pushed_and_job_events_are_admin_only() {
    if mbl_probe::FfTools::locate("", "").await.is_err() {
        eprintln!("skipping: needs ffprobe");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media");
    std::fs::create_dir_all(&media).unwrap();
    let mut config = Config::default();
    config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
    let stop = CancellationToken::new();
    let (router, _db) =
        mbl_server::build(config).await.unwrap().start_jobs(stop.clone()).await.unwrap();
    let app = TestApp { router: router.clone(), dir };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, service).await });

    let admin = app.setup_admin().await;
    let user = app.create_user(&admin, "viewer").await;
    let lib = app
        .call(
            axum::http::Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(json!({ "name": "M", "kind": "movie", "paths": [media.to_str().unwrap()] })),
        )
        .await;
    let lib_id = lib.body["id"].as_i64().unwrap();

    let mut admin_ws = connect(addr, &admin).await;
    let mut user_ws = connect(addr, &user).await;
    let res = app
        .call(
            axum::http::Method::POST,
            &format!("/api/v1/libraries/{lib_id}/scan"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::ACCEPTED, "{:?}", res.body);

    let scan_done = |v: &Value| v["type"] == "scanProgress" && v["data"]["done"] == true;
    let admin_events = collect_until(&mut admin_ws, scan_done).await;
    assert!(admin_events.iter().any(|v| v["type"] == "jobUpdated"), "{admin_events:?}");
    assert_eq!(admin_events.last().unwrap()["data"]["libraryId"], lib_id);

    let user_events = collect_until(&mut user_ws, scan_done).await;
    assert!(user_events.iter().all(|v| v["type"] != "jobUpdated"), "{user_events:?}");
    stop.cancel();
}

#[tokio::test]
async fn cross_origin_cookie_upgrade_is_rejected() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let res = app
        .send(
            Request::get("/api/v1/events")
                .header(header::COOKIE, format!("mbl_session={admin}"))
                .header(header::HOST, "mbl.local:8420")
                .header(header::ORIGIN, "https://evil.example")
                .header(header::UPGRADE, "websocket")
                .header(header::CONNECTION, "upgrade")
                .header("sec-websocket-version", "13")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    let anon = app.call(axum::http::Method::GET, "/api/v1/events", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
}
