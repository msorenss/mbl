mod common;

use axum::http::{Method, StatusCode, header};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn only_admins_can_read_recent_logs_and_http_entries_use_route_templates() {
    const LOGS: &str = "/api/v1/system/logs";
    let app = TestApp::new().await;
    assert_eq!(app.call(Method::GET, LOGS, None, None).await.status, StatusCode::UNAUTHORIZED);

    let admin = app.setup_admin().await;
    let member = app.create_user(&admin, "member").await;
    assert_eq!(
        app.call(Method::GET, LOGS, Some(&member), None).await.status,
        StatusCode::FORBIDDEN
    );

    let missing = app
        .call(Method::GET, "/api/v1/items/987654321?token=secret-query", Some(&admin), None)
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let logs = app.call(Method::GET, LOGS, Some(&admin), None).await;
    assert_eq!(logs.status, StatusCode::OK);
    assert_eq!(logs.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(logs.body[0]["message"], json!("GET /api/v1/items/{id} → 404"));
    assert_eq!(logs.body[0]["level"], json!("warn"));
    let exported = logs.body.to_string();
    for hidden in ["secret-query", "987654321", admin.as_str(), member.as_str(), LOGS] {
        assert!(!exported.contains(hidden));
    }
}
