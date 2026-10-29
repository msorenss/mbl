//! Regressions for the post-GA ownership and resource-lifecycle audit.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn player_presence_and_commands_are_owner_bound() {
    let (app, _, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let alice = app.create_user(&admin, "alice").await;
    let bob = app.create_user(&admin, "bob").await;
    let body = json!({"id":"alice-tv", "name":"TV", "deviceType":"web", "state":"paused", "positionMs":0, "durationMs":1000, "volume":50});
    assert_eq!(
        app.call(Method::POST, "/api/v1/players", Some(&alice), Some(body.clone())).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.call(Method::POST, "/api/v1/players", Some(&bob), Some(body)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.call(Method::GET, "/api/v1/players", Some(&bob), None).await.body, json!([]));
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/players/alice-tv/command",
            Some(&bob),
            Some(json!({"action":"pause"}))
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/players/alice-tv/command",
            Some(&alice),
            Some(json!({"action":"pause"}))
        )
        .await
        .status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        app.call(Method::DELETE, "/api/v1/players/alice-tv", Some(&bob), None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn companion_requires_an_existing_authorized_playback() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let bob = app.create_user(&admin, "bob").await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;
    let result = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId":fixture.movie_id,"profile":"chrome"})),
        )
        .await;
    assert_eq!(result.status, StatusCode::OK);
    let id = result.body["sessionId"].as_str().unwrap();
    for session in [id, "invented-session"] {
        assert_eq!(
            app.call(
                Method::GET,
                &format!("/api/v1/companion/sessions/{session}/context"),
                Some(&bob),
                None
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            app.call(
                Method::POST,
                &format!("/api/v1/companion/sessions/{session}/join"),
                Some(&bob),
                Some(json!({"botName":"bot"}))
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        app.call(Method::POST, &format!("/api/v1/playback/{id}/stop"), Some(&admin), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.call(
            Method::GET,
            &format!("/api/v1/companion/sessions/{id}/context"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn live_reservations_cannot_be_renewed_or_released_by_another_session() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let bob = app.create_user(&admin, "bob").await;
    let source = common::live_source().await;
    let import = app.call(Method::POST,"/api/v1/livetv/tuners/m3u",Some(&admin),Some(json!({"name":"Tuner","content":format!("#EXTM3U\n#EXTINF:-1,Test\n{}\n",source.url)}))).await;
    assert_eq!(import.status, StatusCode::OK);
    let channels = app.call(Method::GET, "/api/v1/livetv/channels", Some(&admin), None).await;
    let channel = channels.body[0]["id"].as_i64().unwrap();
    sqlx::query("UPDATE tuners SET tuner_count = 1").execute(&db.write).await.unwrap();
    let uri = format!("/api/v1/livetv/stream/{channel}");
    let stream = app.call(Method::GET, &uri, Some(&admin), None).await;
    assert_eq!(stream.status, StatusCode::OK);
    let lease = stream.body["sessionId"].as_str().unwrap();
    assert_eq!(app.call(Method::GET, &uri, Some(&bob), None).await.status, StatusCode::CONFLICT);
    assert_eq!(
        app.call(Method::POST, &format!("{uri}/release?sessionId={lease}"), Some(&bob), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call(Method::GET, &format!("{uri}?sessionId={lease}"), Some(&bob), None).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call(Method::GET, &format!("{uri}?sessionId={lease}"), Some(&admin), None).await.status,
        StatusCode::OK
    );
    assert_eq!(
        app.call(Method::POST, &format!("{uri}/release?sessionId={lease}"), Some(&admin), None)
            .await
            .status,
        StatusCode::OK
    );
    let next = app.call(Method::GET, &uri, Some(&bob), None).await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.body);
    assert_eq!(
        app.call(
            Method::POST,
            &format!("{uri}/release?sessionId={}", next.body["sessionId"].as_str().unwrap()),
            Some(&bob),
            None
        )
        .await
        .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn registered_player_receives_commands_over_its_websocket() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
    let (app, _, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let body = json!({"id":"websocket-tv", "name":"TV", "deviceType":"web", "state":"paused", "positionMs":0, "durationMs":1000, "volume":50});
    assert_eq!(
        app.call(Method::POST, "/api/v1/players", Some(&admin), Some(body)).await.status,
        StatusCode::NO_CONTENT
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = app.router.clone().into_make_service_with_connect_info::<std::net::SocketAddr>();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut request = format!("ws://{address}/api/v1/players/websocket-tv/connect")
        .into_client_request()
        .unwrap();
    request.headers_mut().insert("Authorization", format!("Bearer {admin}").parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let command = json!({"action":"seek","params":{"positionMs":321}});
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/players/websocket-tv/command",
            Some(&admin),
            Some(command.clone())
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let received = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(frame) = socket.next().await {
            if let Message::Text(text) = frame.unwrap() {
                return serde_json::from_str::<serde_json::Value>(&text).unwrap();
            }
        }
        panic!("command socket closed");
    })
    .await
    .unwrap();
    assert_eq!(received, command);
    socket.send(Message::Close(None)).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if app.call(Method::GET, "/api/v1/players", Some(&admin), None).await.body == json!([])
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    server.abort();
}
