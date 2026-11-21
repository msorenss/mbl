//! Hailo model installer: admin only, a fixed list of files, and a clear reason when the
//! machine cannot use them.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;

#[tokio::test]
async fn the_model_installer_lists_known_models_for_admins() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;

    let res = app.call(Method::GET, "/api/v1/hw/models", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    let res = app.call(Method::GET, "/api/v1/hw/models", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let files: Vec<&str> = res.body["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["file"].as_str().unwrap())
        .collect();
    assert!(files.contains(&"Qwen2-VL-2B-Instruct.hef") && files.contains(&"Whisper-Base.hef"));
    assert!(res.body["directory"].as_str().unwrap().ends_with("models"), "{}", res.body);
    // Without a Hailo-10H (test machines) the reason is given and nothing can be downloaded.
    if res.body["supported"] == false {
        assert!(res.body["reason"].is_string());
        let res = app
            .call(Method::POST, "/api/v1/hw/models/Whisper-Base.hef/download", Some(&admin), None)
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    }

    // Only files from the list.
    let res =
        app.call(Method::POST, "/api/v1/hw/models/evil.hef/download", Some(&admin), None).await;
    assert!(matches!(res.status, StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY));
    let res =
        app.call(Method::DELETE, "/api/v1/hw/models/Whisper-Base.hef", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND, "not installed");
    let res =
        app.call(Method::DELETE, "/api/v1/hw/models/Whisper-Base.hef", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // The NPU switch: admin only, saved, and off means nothing is held.
    let res = app.call(Method::GET, "/api/v1/hw/npu", Some(&viewer), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let off = serde_json::json!({ "enabled": false });
    let res = app.call(Method::PUT, "/api/v1/hw/npu", Some(&admin), Some(off)).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["enabled"], false);
    assert_eq!(res.body["detectionLoaded"], false);
    assert_eq!(res.body["descriptionLoaded"], false);
    let on = serde_json::json!({ "enabled": true });
    let res = app.call(Method::PUT, "/api/v1/hw/npu", Some(&admin), Some(on)).await;
    assert_eq!(res.body["enabled"], true);
    let res = app.call(Method::GET, "/api/v1/hw/npu", Some(&admin), None).await;
    assert_eq!(res.body["enabled"], true);
}
