//! Integration tests for Milestone M8: Distributed Nodes
//! - M8.1 & M8.2: Transcode Worker pairing code & registration flow
//! - M8.3 & M8.4: Source media range access with signed HMAC token & segment PUT upload
//! - M8.5 & M8.7: Worker heartbeats, Wake-on-LAN magic packet, gaming pause state
//! - M8.9: Worker & Remote Node MCP tools
//! - M8.10: Standalone Storage Node pairing & delta catalog sync
//! - M8.11: Smart handover for LAN direct playback vs WAN relay

#![allow(clippy::unwrap_used)]

mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use mbl_core::{PartId, WorkerId};
use mbl_mcp::McpClient;
use serde_json::json;

#[tokio::test]
async fn m8_worker_pairing_and_lifecycle() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Generate pairing code as Admin
    let res =
        app.call(Method::POST, "/api/v1/workers/pairing-code", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let code = res.body["code"].as_str().unwrap().to_owned();
    assert!(code.starts_with("W-"));
    assert_eq!(code.len(), 8);

    // 2. Submit Worker Pair Request using code
    let pair_res = app
        .call(
            Method::POST,
            "/api/v1/workers/pair",
            None,
            Some(json!({
                "pairingCode": code,
                "name": "Gaming PC (RTX 4080)",
                "hostname": "desktop-gaming",
                "macAddress": "00:1A:2B:3C:4D:5E",
                "capabilities": {
                    "gpuVendor": "Nvidia",
                    "gpuDeviceName": "NVIDIA GeForce RTX 4080",
                    "supportedVideoDecoders": ["h264", "hevc", "av1"],
                    "supportedVideoEncoders": ["h264_nvenc", "hevc_nvenc", "av1_nvenc"],
                    "supportsHardwareTonemap": true,
                    "maxConcurrentSessions": 4
                }
            })),
        )
        .await;
    assert_eq!(pair_res.status, StatusCode::OK);
    let worker_id = pair_res.body["workerId"].as_i64().unwrap();
    let worker_token = pair_res.body["authToken"].as_str().unwrap().to_owned();
    assert!(worker_token.starts_with("mbl_wrk_"));

    // Code is single-use: reusing fails
    let reuse_res = app
        .call(
            Method::POST,
            "/api/v1/workers/pair",
            None,
            Some(json!({
                "pairingCode": code,
                "name": "Impostor",
                "capabilities": {
                    "gpuVendor": "None",
                    "gpuDeviceName": "None",
                    "supportedVideoDecoders": [],
                    "supportedVideoEncoders": [],
                    "supportsHardwareTonemap": false,
                    "maxConcurrentSessions": 1
                }
            })),
        )
        .await;
    assert_eq!(reuse_res.status, StatusCode::UNAUTHORIZED);

    // 3. Worker sends periodic Heartbeat
    let hb_res = app
        .call(
            Method::POST,
            "/api/v1/workers/heartbeat",
            Some(&worker_token),
            Some(json!({
                "workerId": worker_id,
                "status": {
                    "state": "idle",
                    "activeSessions": 0,
                    "cpuUsagePercent": 4.5,
                    "gpuUsagePercent": 8.0
                },
                "activeJobIds": []
            })),
        )
        .await;
    assert_eq!(hb_res.status, StatusCode::NO_CONTENT);

    // 4. Admin lists workers
    let list_res = app.call(Method::GET, "/api/v1/workers", Some(&admin_token), None).await;
    assert_eq!(list_res.status, StatusCode::OK);
    let workers = list_res.body.as_array().unwrap();
    assert_eq!(workers.len(), 1);
    assert_eq!(workers[0]["name"], "Gaming PC (RTX 4080)");
    assert_eq!(workers[0]["status"], "idle");
    assert_eq!(workers[0]["macAddress"], "00:1A:2B:3C:4D:5E");

    // 5. Admin sends Wake-on-LAN magic packet
    let wake_res = app
        .call(Method::POST, &format!("/api/v1/workers/{worker_id}/wake"), Some(&admin_token), None)
        .await;
    assert_eq!(wake_res.status, StatusCode::NO_CONTENT);

    // 6. Admin revokes worker
    let del_res = app
        .call(Method::DELETE, &format!("/api/v1/workers/{worker_id}"), Some(&admin_token), None)
        .await;
    assert_eq!(del_res.status, StatusCode::NO_CONTENT);

    // After revoke, worker heartbeat is rejected
    let dead_hb = app
        .call(
            Method::POST,
            "/api/v1/workers/heartbeat",
            Some(&worker_token),
            Some(json!({
                "workerId": worker_id,
                "status": {
                    "state": "idle",
                    "activeSessions": 0,
                    "cpuUsagePercent": 0.0
                },
                "activeJobIds": []
            })),
        )
        .await;
    assert_eq!(dead_hb.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn m8_worker_source_token_and_segment_upload() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Create a media file fixture
    let media_dir = app.media_dir("Movies");
    let movie_file = format!("{media_dir}/sample.mp4");
    tokio::fs::write(&movie_file, b"sample mp4 video contents for worker range streaming")
        .await
        .unwrap();

    let lib_res = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_token),
            Some(json!({ "name": "Movies", "kind": "movie", "paths": [media_dir] })),
        )
        .await;
    assert_eq!(lib_res.status, StatusCode::CREATED);
    let lib_id = lib_res.body["id"].as_i64().unwrap();

    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'movie', 'Test Movie', 'test movie', 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ver_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mp4', 'h264', 'aac', 1920, 1080) RETURNING id",
    )
    .bind(item_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let part_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, part_index, probe)
         VALUES (?, ?, 53, 1, 60000, 0, '{}') RETURNING id",
    )
    .bind(ver_id)
    .bind(&movie_file)
    .fetch_one(&db.write)
    .await
    .unwrap();

    // 2. Pair a worker
    let code_res =
        app.call(Method::POST, "/api/v1/workers/pairing-code", Some(&admin_token), None).await;
    let code = code_res.body["code"].as_str().unwrap();

    let pair_res = app
        .call(
            Method::POST,
            "/api/v1/workers/pair",
            None,
            Some(json!({
                "pairingCode": code,
                "name": "GPU Worker 1",
                "capabilities": {
                    "gpuVendor": "Nvidia",
                    "gpuDeviceName": "RTX 3070",
                    "supportedVideoDecoders": ["h264"],
                    "supportedVideoEncoders": ["h264_nvenc"],
                    "supportsHardwareTonemap": true,
                    "maxConcurrentSessions": 2
                }
            })),
        )
        .await;
    let worker_id = WorkerId(pair_res.body["workerId"].as_i64().unwrap());
    let worker_token = pair_res.body["authToken"].as_str().unwrap().to_owned();

    // 3. Central server signs source URL token for worker
    let signed_token = services.workers.generate_source_token(worker_id, PartId(part_id), 3600);

    // 4. Worker requests source media with token and Range header
    let stream_url = format!("/api/v1/stream/worker/source/{part_id}?token={signed_token}");
    let req = Request::builder()
        .method(Method::GET)
        .uri(&stream_url)
        .header(header::RANGE, "bytes=0-15")
        .body(Body::empty())
        .unwrap();

    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers.get(header::CONTENT_RANGE).unwrap(), "bytes 0-15/52");

    // 5. Worker uploads finished segment chunk via PUT
    let upload_url = "/api/v1/stream/hls/sess_test_123/segments/segment_00000.m4s";
    let segment_bytes = b"fMP4_SEGMENT_DATA_TEST_BYTES";

    let put_req = Request::builder()
        .method(Method::PUT)
        .uri(upload_url)
        .header(header::AUTHORIZATION, format!("Bearer {worker_token}"))
        .header(header::CONTENT_TYPE, "video/iso.segment")
        .body(Body::from(&segment_bytes[..]))
        .unwrap();

    let put_res = app.send(put_req).await;
    assert_eq!(put_res.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn m8_remote_node_federation_and_handover() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Generate node pairing code
    let code_res =
        app.call(Method::POST, "/api/v1/nodes/pairing-code", Some(&admin_token), None).await;
    assert_eq!(code_res.status, StatusCode::OK);
    let code = code_res.body["code"].as_str().unwrap().to_owned();
    assert!(code.starts_with("N-"));

    // 2. Submit Remote Node pair request
    let pair_res = app
        .call(
            Method::POST,
            "/api/v1/nodes/pair",
            None,
            Some(json!({
                "pairingCode": code,
                "name": "Basement NAS",
                "baseUrl": "http://192.168.1.150:8430",
                "capabilities": {
                    "gpuVendor": "Intel",
                    "supportedVideoEncoders": ["h264_qsv"],
                    "supportsHardwareTranscode": true,
                    "storageTotalBytes": 16000000000000i64,
                    "storageAvailableBytes": 8500000000000i64
                }
            })),
        )
        .await;
    assert_eq!(pair_res.status, StatusCode::OK);
    let node_id = pair_res.body["nodeId"].as_i64().unwrap();
    let node_token = pair_res.body["authToken"].as_str().unwrap().to_owned();
    assert!(node_token.starts_with("mbl_nod_"));

    // 3. Storage Node submits Delta Catalog Sync
    let sync_res = app
        .call(
            Method::POST,
            &format!("/api/v1/nodes/{node_id}/sync-catalog"),
            Some(&node_token),
            Some(json!({
                "libraries": [
                    {
                        "remoteLibraryId": 101,
                        "name": "NAS 4K Movies",
                        "kind": "movies",
                        "itemCount": 450
                    },
                    {
                        "remoteLibraryId": 102,
                        "name": "NAS TV Shows",
                        "kind": "tv",
                        "itemCount": 1820
                    }
                ]
            })),
        )
        .await;
    assert_eq!(sync_res.status, StatusCode::OK);
    assert_eq!(sync_res.body["syncedLibraries"], 2);
    assert_eq!(sync_res.body["totalItems"], 2270);

    // 4. Admin lists node libraries
    let lib_list = app
        .call(Method::GET, &format!("/api/v1/nodes/{node_id}/libraries"), Some(&admin_token), None)
        .await;
    assert_eq!(lib_list.status, StatusCode::OK);
    let libs = lib_list.body.as_array().unwrap();
    assert_eq!(libs.len(), 2);
    assert_eq!(libs[0]["name"], "NAS 4K Movies");

    // 5. Smart handover test:
    // Case A: LAN client IP -> Direct Play handover granted
    let mut lan_req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/nodes/{node_id}/stream/handover/999"))
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    lan_req
        .extensions_mut()
        .insert(ConnectInfo("192.168.1.75:51234".parse::<SocketAddr>().unwrap()));

    let lan_res = app.send(lan_req).await;
    assert_eq!(lan_res.status, StatusCode::OK);
    assert_eq!(lan_res.body["requiresRelay"], false);
    assert_eq!(lan_res.body["directPlayUrl"], "http://192.168.1.150:8430/api/v1/stream/direct/999");

    // Case B: WAN client IP -> Requires Relay
    let mut wan_req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/nodes/{node_id}/stream/handover/999"))
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();
    wan_req
        .extensions_mut()
        .insert(ConnectInfo("203.0.113.42:51234".parse::<SocketAddr>().unwrap()));

    let wan_res = app.send(wan_req).await;
    assert_eq!(wan_res.status, StatusCode::OK);
    assert_eq!(wan_res.body["requiresRelay"], true);
    assert_eq!(wan_res.body["directPlayUrl"], serde_json::Value::Null);
}

#[tokio::test]
async fn m8_mcp_tools_workers_and_nodes() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin_session = app.setup_admin().await;

    // Mint admin token
    let tok_res = app
        .call(
            Method::POST,
            "/api/v1/auth/tokens",
            Some(&admin_session),
            Some(json!({
                "name": "worker-admin-bot",
                "scopes": ["admin"]
            })),
        )
        .await;
    assert_eq!(tok_res.status, StatusCode::CREATED);
    let token = tok_res.body["token"].as_str().unwrap().to_owned();

    let mut client = McpClient::connect_router(app.router.clone(), Some(token));
    let _init = client.initialize().await.unwrap();

    // 1. Tool: pair_worker
    let pair_tool = client.call_tool("pair_worker", json!({})).await.unwrap();
    assert!(!pair_tool.is_error);
    let code = pair_tool.structured_content.as_ref().unwrap()["pairingCode"].as_str().unwrap();
    assert!(code.starts_with("W-"));

    // 2. Tool: list_workers
    let list_workers = client.call_tool("list_workers", json!({})).await.unwrap();
    assert!(!list_workers.is_error);

    // 3. Tool: list_remote_nodes
    let list_nodes = client.call_tool("list_remote_nodes", json!({})).await.unwrap();
    assert!(!list_nodes.is_error);

    // 4. Create library & item, then test create_optimized_versions with dry_run
    let media_dir = app.media_dir("MoviesOpt");
    let lib_res = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_session),
            Some(json!({ "name": "MoviesOpt", "kind": "movie", "paths": [media_dir] })),
        )
        .await;
    let lib_id = lib_res.body["id"].as_i64().unwrap();
    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (?, 'movie', 'Opt Movie', 'opt movie', 100, 100) RETURNING id",
    )
    .bind(lib_id)
    .fetch_one(&db.write)
    .await
    .unwrap();
    let ver_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mp4', 'h264', 'aac', 1920, 1080) RETURNING id",
    )
    .bind(item_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let _part_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, part_index, probe)
         VALUES (?, '/opt/test.mp4', 1024, 1, 60000, 0, '{}') RETURNING id",
    )
    .bind(ver_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let opt_tool = client
        .call_tool(
            "create_optimized_versions",
            json!({
                "item_id": item_id,
                "dry_run": true,
                "profile": "universal_1080p"
            }),
        )
        .await
        .unwrap();
    assert!(!opt_tool.is_error);
    assert_eq!(opt_tool.structured_content.as_ref().unwrap()["dryRun"], true);

    // 5. Tool: revoke_worker with dry_run
    let revoke_dry = client
        .call_tool(
            "revoke_worker",
            json!({
                "worker_id": 999,
                "dry_run": true
            }),
        )
        .await
        .unwrap();
    // 999 not found -> NotFound error
    assert!(revoke_dry.is_error);
}
