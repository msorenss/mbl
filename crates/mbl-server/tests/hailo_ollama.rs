//! Hailo-Ollama from the Hardware page, against a stand-in server: its catalog, a download with
//! progress, and removing a model.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn models_are_listed_downloaded_and_removed() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;

    let installed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let (pulled, deleted) = (installed.clone(), installed.clone());
    let listed = installed.clone();
    let router = axum::Router::new()
        .route("/api/version", axum::routing::get(|| async { axum::Json(json!({"version": "0.5.1"})) }))
        .route(
            "/hailo/v1/list",
            axum::routing::get(|| async {
                axum::Json(json!({"models": ["qwen2.5:1.5b", "llama3.2:1b"]}))
            }),
        )
        .route(
            "/api/tags",
            axum::routing::get(move || {
                let names = listed.lock().unwrap().clone();
                async move {
                    axum::Json(json!({"models": names.iter().map(|n| json!({"name": n, "size": 1_900_000_000u64})).collect::<Vec<_>>()}))
                }
            }),
        )
        .route(
            "/api/pull",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let pulled = pulled.clone();
                async move {
                    pulled.lock().unwrap().push(body["model"].as_str().unwrap().to_owned());
                    "{\"status\":\"pulling\",\"total\":100,\"completed\":50}\n{\"status\":\"success\"}\n"
                }
            }),
        )
        .route(
            "/api/delete",
            axum::routing::delete(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let deleted = deleted.clone();
                async move {
                    deleted.lock().unwrap().retain(|n| n != body["model"].as_str().unwrap());
                    StatusCode::OK
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    mbl_db::repo::settings::SettingsRepo::new(db.clone())
        .set("hailo_ollama_url", &base)
        .await
        .unwrap();

    let res = app.call(Method::GET, "/api/v1/hw/ollama", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["running"], true);
    assert_eq!(res.body["version"], "0.5.1");
    assert_eq!(res.body["assistantUrl"], format!("{base}/v1"));
    assert_eq!(
        res.body["models"][0],
        json!({"name": "qwen2.5:1.5b", "installed": false, "sizeBytes": null})
    );

    let res = app
        .call(Method::POST, "/api/v1/hw/ollama/models/qwen2.5:1.5b/pull", Some(&admin), None)
        .await;
    assert_eq!(res.status, StatusCode::ACCEPTED, "{}", res.body);
    let mut status = serde_json::Value::Null;
    for _ in 0..100 {
        status = app.call(Method::GET, "/api/v1/hw/ollama", Some(&admin), None).await.body;
        if status["pull"]["finished"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(status["pull"]["error"], serde_json::Value::Null, "{status}");
    assert_eq!(status["pull"]["status"], "success");
    assert_eq!(status["pull"]["completed"], 50);
    assert_eq!(status["models"][0]["installed"], true, "{status}");
    assert_eq!(status["models"][0]["sizeBytes"], 1_900_000_000u64);

    // Bad names never reach the server.
    let res =
        app.call(Method::POST, "/api/v1/hw/ollama/models/..%2Fx/pull", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);

    let res =
        app.call(Method::DELETE, "/api/v1/hw/ollama/models/qwen2.5:1.5b", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
    let res = app.call(Method::GET, "/api/v1/hw/ollama", Some(&admin), None).await;
    assert_eq!(res.body["models"][0]["installed"], false);

    let viewer = app.create_user(&admin, "viewer").await;
    let res = app.call(Method::GET, "/api/v1/hw/ollama", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}
