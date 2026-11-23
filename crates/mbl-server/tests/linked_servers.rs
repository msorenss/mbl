//! Linked servers: one MBL controls another's players over its MCP endpoint with a token.

mod common;

use std::net::SocketAddr;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn a_server_controls_and_plays_on_a_linked_servers_players() {
    // Server B, reachable over HTTP, with a player and a read+playback token for A.
    let (b, b_db, _) = TestApp::new_with_services().await;
    let b_admin = b.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&b, &b_db, &b_admin).await;
    let player = json!({"id":"b-tv", "name":"Living room", "deviceType":"web", "state":"paused",
        "positionMs":0, "durationMs":1000, "volume":50});
    let res = b.call(Method::POST, "/api/v1/players", Some(&b_admin), Some(player)).await;
    assert!(res.status.is_success(), "{}", res.body);
    let res = b
        .call(
            Method::POST,
            "/api/v1/auth/tokens",
            Some(&b_admin),
            Some(json!({"name": "server A", "scopes": ["read", "playback"]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    let token = res.body["token"].as_str().unwrap().to_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = b.router.clone();
    let server = tokio::spawn(async move {
        let _ =
            axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await;
    });
    let b_url = format!("http://{addr}");

    // Server A links to B; a wrong token is refused when saving.
    let (a, _, _) = TestApp::new_with_services().await;
    let a_admin = a.setup_admin().await;
    let res = a
        .call(
            Method::POST,
            "/api/v1/system/linked-servers",
            Some(&a_admin),
            Some(json!({"name": "Windows", "url": b_url, "token": "mbl_wrong"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    let res = a
        .call(
            Method::POST,
            "/api/v1/system/linked-servers",
            Some(&a_admin),
            Some(json!({"name": "Windows", "url": format!("{b_url}/"), "token": token})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let id = res.body["id"].as_str().unwrap().to_owned();
    assert_eq!(res.body["url"], b_url);
    assert!(res.body.get("token").is_none());

    // The server menu lists B by name and address without contacting it.
    let res = a.call(Method::GET, "/api/v1/linked-servers/names", Some(&a_admin), None).await;
    assert_eq!(res.body, json!([{"id": id, "name": "Windows", "url": b_url, "shared": false}]));

    // A lists B's players.
    let res = a.call(Method::GET, "/api/v1/linked-servers", Some(&a_admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body[0]["error"], json!(null), "{}", res.body);
    assert_eq!(res.body[0]["players"][0]["id"], "b-tv", "{}", res.body);
    assert_eq!(res.body[0]["players"][0]["name"], "Living room");

    // B's player connects its command channel; A searches B's library, plays a title on the
    // player, pauses and seeks it, and each command arrives on B's player.
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
    let mut request =
        format!("ws://{addr}/api/v1/players/b-tv/connect").into_client_request().unwrap();
    request.headers_mut().insert("Authorization", format!("Bearer {b_admin}").parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let mut next_command = async || {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while let Some(frame) = socket.next().await {
                if let Message::Text(text) = frame.unwrap() {
                    return serde_json::from_str::<serde_json::Value>(&text).unwrap();
                }
            }
            panic!("command socket closed");
        })
        .await
        .unwrap()
    };

    let res = a
        .call(Method::GET, &format!("/api/v1/linked-servers/{id}/search?q=a"), Some(&a_admin), None)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let res = a
        .call(
            Method::POST,
            &format!("/api/v1/linked-servers/{id}/players/b-tv/play"),
            Some(&a_admin),
            Some(json!({"itemId": fixture.movie_id})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
    let play = next_command().await;
    assert_eq!(play["action"], "play", "{play}");
    assert_eq!(play["params"]["itemId"], fixture.movie_id);
    for (body, expected) in [
        (json!({"action": "pause"}), json!({"action": "pause"})),
        (
            json!({"action": "seek", "positionMs": 4321}),
            json!({"action": "seek", "params": {"positionMs": 4321}}),
        ),
    ] {
        let res = a
            .call(
                Method::POST,
                &format!("/api/v1/linked-servers/{id}/players/b-tv/command"),
                Some(&a_admin),
                Some(body),
            )
            .await;
        assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
        assert_eq!(next_command().await, expected);
    }

    let res = a
        .call(
            Method::POST,
            &format!("/api/v1/linked-servers/{id}/players/b-tv/command"),
            Some(&a_admin),
            Some(json!({"action": "seek"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Other accounts on A do not see unshared links; managed profiles never.
    let viewer = a.create_user(&a_admin, "viewer").await;
    let res = a.call(Method::GET, "/api/v1/linked-servers", Some(&viewer), None).await;
    assert_eq!(res.body, json!([]));
    let res = a.call(Method::GET, "/api/v1/linked-servers/names", Some(&viewer), None).await;
    assert_eq!(res.body, json!([]));
    let res = a.call(Method::GET, "/api/v1/system/linked-servers", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // Removing the link.
    let res = a
        .call(Method::DELETE, &format!("/api/v1/system/linked-servers/{id}"), Some(&a_admin), None)
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let res = a.call(Method::GET, "/api/v1/linked-servers", Some(&a_admin), None).await;
    assert_eq!(res.body, json!([]));
    server.abort();
}
