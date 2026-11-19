//! Built-in assistant settings: admin only, key never returned, connection test and model list
//! against a fake OpenAI-compatible provider.

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use common::TestApp;
use serde_json::{Value, json};

async fn fake_provider() -> String {
    async fn chat(headers: HeaderMap, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
        if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer sk-test") {
            return (StatusCode::UNAUTHORIZED, Json(json!({"error": {"message": "bad key"}})));
        }
        if body["tools"].is_array() {
            return (
                StatusCode::OK,
                Json(
                    json!({"model": "tiny", "choices": [{"finish_reason": "tool_calls", "message": {
                        "content": null,
                        "tool_calls": [{"id": "c", "function": {"name": "get_time", "arguments": "{}"}}]
                    }}]}),
                ),
            );
        }
        (
            StatusCode::OK,
            Json(
                json!({"model": "tiny", "choices": [{"finish_reason": "stop", "message": {"content": "OK"}}]}),
            ),
        )
    }
    async fn models() -> Json<Value> {
        Json(json!({"data": [{"id": "tiny"}, {"id": "big"}]}))
    }
    let app =
        Router::new().route("/v1/chat/completions", post(chat)).route("/v1/models", get(models));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn admin_configures_and_tests_the_assistant() {
    let (app, _db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let base = fake_provider().await;

    let res = app.call(Method::GET, "/api/v1/system/assistant", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["enabled"], false);
    assert_eq!(res.body["provider"], "openai_compatible");

    // Turning it on needs an address and a model.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": "", "model": ""})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": base, "model": "tiny", "effort": "turbo"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);

    // Models and the connection test use the key from the form before it is saved.
    let connection = json!({"provider": "openai_compatible", "baseUrl": base, "apiKey": "sk-test", "model": "tiny"});
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/models",
            Some(&admin),
            Some(connection.clone()),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body, json!(["big", "tiny"]));
    let res = app
        .call(Method::POST, "/api/v1/system/assistant/test", Some(&admin), Some(connection))
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["ok"], true, "{}", res.body);
    assert_eq!(res.body["reply"], "OK");
    assert_eq!(res.body["toolsSupported"], true);

    // Save; the key is stored but never returned.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": format!("{base}/"), "apiKey": "sk-test", "model": "tiny", "effort": "Low"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["hasApiKey"], true);
    assert_eq!(res.body["baseUrl"], base);
    assert_eq!(res.body["effort"], "low");
    assert!(!res.body.to_string().contains("sk-test"));

    // Without a key in the form the stored one is used; a wrong key is reported, not thrown.
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(&admin),
            Some(json!({"provider": "openai_compatible", "baseUrl": base, "model": "tiny"})),
        )
        .await;
    assert_eq!(res.body["ok"], true, "{}", res.body);
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(&admin),
            Some(json!({"provider": "openai_compatible", "baseUrl": base, "apiKey": "wrong", "model": "tiny"})),
        )
        .await;
    assert_eq!(res.body["ok"], false);
    assert!(res.body["error"].as_str().unwrap().contains("rejected"), "{}", res.body);

    // The saved provider is ready for the assistant to use.
    let provider = services.assistant.provider().await.unwrap().expect("turned on");
    assert_eq!(provider.config().model, "tiny");
    assert_eq!(provider.config().api_key.as_deref(), Some("sk-test"));

    // Clearing the key works for local servers.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": false, "provider": "anthropic", "baseUrl": "https://api.anthropic.com/v1", "clearApiKey": true, "model": "claude-opus-5-5"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["hasApiKey"], false);
    assert_eq!(res.body["baseUrl"], "https://api.anthropic.com");
    assert!(services.assistant.provider().await.unwrap().is_none());

    // Other users cannot read or change it.
    let user = app.create_user(&admin, "viewer").await;
    for (method, path, body) in [
        (Method::GET, "/api/v1/system/assistant", None),
        (
            Method::PUT,
            "/api/v1/system/assistant",
            Some(json!({"enabled": false, "provider": "anthropic", "baseUrl": "", "model": ""})),
        ),
        (
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(json!({"provider": "anthropic", "baseUrl": "https://x", "model": "m"})),
        ),
        (
            Method::POST,
            "/api/v1/system/assistant/models",
            Some(json!({"provider": "anthropic", "baseUrl": "https://x"})),
        ),
    ] {
        let res = app.call(method, path, Some(&user), body).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}: {}", res.body);
    }
}
