//! A real `mbl-node` shares a folder: its files appear as an MBL library, play directly
//! (relayed) and through HLS transcoding, and disappear when removed or when the node is revoked.
#![allow(clippy::unwrap_used)]
mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use mbl_core::{LibraryKind, NodeId};
use serde_json::json;

async fn make_clip(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=s=160x90:r=24:d=8"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=8"])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "24", "-c:a", "aac"])
        .arg(path)
        .status()
        .await
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn node_folder_becomes_a_playable_library() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;

    // The node's shared folder, outside every server library root.
    let share = tempfile::tempdir().unwrap();
    let clip = share.path().join("Heat (1995)").join("Heat (1995).mp4");
    make_clip(&clip).await;

    let node_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_addr = node_listener.local_addr().unwrap();
    let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_listener.local_addr().unwrap();
    let service = app.router.clone().into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(server_listener, service).await });

    // Pair exactly like `mbl-node pair`.
    let code = app.call(Method::POST, "/api/v1/nodes/pairing-code", Some(&admin), None).await;
    let paired = app
        .call(
            Method::POST,
            "/api/v1/nodes/pair",
            None,
            Some(json!({
                "pairingCode": code.body["code"],
                "name": "test node",
                "baseUrl": format!("http://{node_addr}"),
                "capabilities": {"supportedVideoEncoders": [], "supportsHardwareTranscode": false}
            })),
        )
        .await;
    assert_eq!(paired.status, StatusCode::OK, "{}", paired.body);
    let mut config = mbl_node::NodeConfigFile {
        server_url: format!("http://{server_addr}"),
        node_id: NodeId(paired.body["nodeId"].as_i64().unwrap()),
        auth_token: paired.body["authToken"].as_str().unwrap().to_owned(),
        name: "test node".into(),
        base_url: format!("http://{node_addr}"),
        stream_secret: paired.body["streamSecret"].as_str().map(str::to_owned),
        listen: None,
        libraries: Vec::new(),
    };
    assert!(config.stream_secret.is_some());
    config.add_library("Movies", LibraryKind::Movie, share.path()).unwrap();
    let router = mbl_node::file_router(&config).unwrap();
    tokio::spawn(async move { axum::serve(node_listener, router).await });

    let tools = mbl_probe::FfTools::locate("", "").await.unwrap();
    let client = reqwest::Client::new();
    assert_eq!(mbl_node::sync_once(&config, &tools, &client).await.unwrap(), 1);
    // A second sync finds nothing new.
    assert_eq!(mbl_node::sync_once(&config, &tools, &client).await.unwrap(), 0);

    let libraries = app.call(Method::GET, "/api/v1/libraries", Some(&admin), None).await;
    let library = libraries
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "Movies (test node)")
        .unwrap()
        .clone();
    let items = app
        .call(
            Method::GET,
            &format!("/api/v1/libraries/{}/items", library["id"]),
            Some(&admin),
            None,
        )
        .await;
    assert!(items.body.to_string().contains("Heat"), "{}", items.body);
    let (part_id, item_id): (i64, i64) = sqlx::query_as(
        "SELECT p.id, v.item_id FROM media_parts p JOIN media_versions v ON v.id = p.version_id
         WHERE p.path LIKE 'mblnode://%'",
    )
    .fetch_one(&db.read)
    .await
    .unwrap();

    // Direct play is relayed byte for byte, with ranges.
    let request = Request::builder()
        .uri(format!("/api/v1/stream/direct/{part_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .header(header::RANGE, "bytes=0-15")
        .body(Body::empty())
        .unwrap();
    let relayed = app.send(request).await;
    assert_eq!(relayed.status, StatusCode::PARTIAL_CONTENT);
    let total = std::fs::metadata(&clip).unwrap().len();
    assert_eq!(relayed.headers[header::CONTENT_RANGE], format!("bytes 0-15/{total}").as_str());

    // Transcoding reads the node file through a signed URL.
    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId": item_id, "waitForReady": true, "maxBitrate": 300000})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    assert_eq!(start.body["method"], "transcode", "{}", start.body);
    let session = services.stream.hls_session(start.body["sessionId"].as_str().unwrap()).unwrap();
    let fragment = session.request_segment("0", 0).await.unwrap();
    assert!(std::fs::metadata(fragment).unwrap().len() > 0);
    app.call(
        Method::POST,
        &format!("/api/v1/playback/{}/stop", start.body["sessionId"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;

    // LAN handover gives a signed URL straight to the node; forged or altered URLs are refused.
    let handover = app
        .call(
            Method::GET,
            &format!("/api/v1/nodes/{}/stream/handover/{item_id}", config.node_id.0),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(handover.status, StatusCode::OK, "{}", handover.body);
    let direct = handover.body["directPlayUrl"].as_str().unwrap().to_owned();
    assert!(direct.starts_with(&format!("http://{node_addr}/files/1/Heat%20(1995)/")), "{direct}");
    assert_eq!(client.head(&direct).send().await.unwrap().status(), 200);
    let forged = direct.replace("sig=", "sig=AA");
    assert_eq!(client.head(&forged).send().await.unwrap().status(), 403);
    let unsigned = format!("http://{node_addr}/files/1/Heat%20(1995)/Heat%20(1995).mp4");
    assert!(client.head(&unsigned).send().await.unwrap().status().is_client_error());

    // Removing the file marks it missing at the next sync.
    std::fs::remove_file(&clip).unwrap();
    mbl_node::sync_once(&config, &tools, &client).await.unwrap();
    let missing: Option<i64> =
        sqlx::query_scalar("SELECT missing_since FROM media_parts WHERE id = ?")
            .bind(part_id)
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert!(missing.is_some());

    // Revoking the node removes its library and refuses its token.
    let revoke = app
        .call(Method::DELETE, &format!("/api/v1/nodes/{}", config.node_id.0), Some(&admin), None)
        .await;
    assert_eq!(revoke.status, StatusCode::NO_CONTENT);
    let libraries = app.call(Method::GET, "/api/v1/libraries", Some(&admin), None).await;
    assert!(!libraries.body.to_string().contains("test node"));
    assert!(mbl_node::sync_once(&config, &tools, &client).await.is_err());
}
