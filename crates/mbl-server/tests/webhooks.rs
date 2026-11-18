//! Webhook delivery: event mapping, HMAC signature, event filter and the admin test call.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::post;
use common::{TestApp, seed_playback_fixtures};
use hmac::{Hmac, KeyInit, Mac};
use mbl_core::events::Event;
use mbl_core::{ItemId, UserId};
use serde_json::{Value, json};

type Received = Arc<Mutex<Vec<(String, Option<String>, Value)>>>;

/// A local receiver that records event name, signature and body.
async fn receiver(status: StatusCode) -> (String, Received) {
    let received: Received = Arc::default();
    let store = received.clone();
    let app = Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: String| {
            let store = store.clone();
            async move {
                let header =
                    |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
                store.lock().unwrap().push((
                    header("x-mbl-event").unwrap_or_default(),
                    header("x-mbl-signature"),
                    serde_json::from_str(&body).unwrap(),
                ));
                status
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/hook"), received)
}

async fn wait_for(received: &Received, count: usize) {
    for _ in 0..100 {
        if received.lock().unwrap().len() >= count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("expected {count} deliveries, got {}", received.lock().unwrap().len());
}

#[tokio::test]
async fn playback_event_is_delivered_signed_and_filtered() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let s = seed_playback_fixtures(&app, &db, &admin).await;
    let (url, received) = receiver(StatusCode::OK).await;

    let res = app
        .call(
            Method::POST,
            "/api/v1/webhooks",
            Some(&admin),
            Some(json!({ "url": url, "name": "HA", "events": ["playback.stopped"], "secret": "s3cret" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    assert_eq!(res.body["secret"], "********");
    let bad = app
        .call(
            Method::POST,
            "/api/v1/webhooks",
            Some(&admin),
            Some(json!({ "url": url, "name": "x", "events": ["nonsense"] })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);

    let user = UserId(
        app.call(Method::GET, "/api/v1/auth/me", Some(&admin), None).await.body["id"]
            .as_i64()
            .unwrap(),
    );
    let events = [
        Event::PlaybackStarted { user_id: user, item_id: ItemId(s.ep1_id), session_id: "a".into() },
        Event::PlaybackStopped {
            user_id: user,
            item_id: ItemId(s.ep1_id),
            session_id: "a".into(),
            position_ms: 1_000,
            duration_ms: 2_000,
            completed: false,
        },
    ];
    for event in &events {
        for (name, payload) in mbl_server::webhooks::payloads(&db, event).await {
            services.webhooks.dispatch_event(name, &payload).await;
        }
    }
    wait_for(&received, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let got = received.lock().unwrap().clone();
    assert_eq!(got.len(), 1, "only the subscribed event is sent: {got:?}");
    let (event, signature, body) = &got[0];
    assert_eq!(event, "playback.stopped");
    assert_eq!(body["item"]["id"], s.ep1_id);
    assert_eq!(body["item"]["kind"], "episode");
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["positionMs"], 1_000);

    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"s3cret").unwrap();
    mac.update(body.to_string().as_bytes());
    let expected: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(signature.as_deref(), Some(format!("sha256={expected}").as_str()));
}

#[tokio::test]
async fn test_call_reports_receiver_status() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (ok_url, received) = receiver(StatusCode::NO_CONTENT).await;
    let (bad_url, _) = receiver(StatusCode::INTERNAL_SERVER_ERROR).await;
    let mut ids = Vec::new();
    for url in [ok_url, bad_url] {
        let res = app
            .call(
                Method::POST,
                "/api/v1/webhooks",
                Some(&admin),
                Some(json!({ "url": url, "name": "t", "events": ["*"] })),
            )
            .await;
        ids.push(res.body["id"].as_i64().unwrap());
    }
    let ok = app
        .call(Method::POST, &format!("/api/v1/webhooks/{}/test", ids[0]), Some(&admin), None)
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body["ok"], true);
    assert_eq!(ok.body["status"], 204);
    assert_eq!(received.lock().unwrap()[0].0, "test");
    let bad = app
        .call(Method::POST, &format!("/api/v1/webhooks/{}/test", ids[1]), Some(&admin), None)
        .await;
    assert_eq!(bad.body["ok"], false);
    assert_eq!(bad.body["status"], 500);

    let user = app.create_user(&admin, "viewer").await;
    let denied = app
        .call(Method::POST, &format!("/api/v1/webhooks/{}/test", ids[0]), Some(&user), None)
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
}
