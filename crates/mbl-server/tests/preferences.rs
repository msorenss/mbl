//! User preferences: the edited menu is saved per user and validated.

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn the_menu_layout_is_saved_per_user() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;

    let res = app.call(Method::GET, "/api/v1/users/me/preferences", Some(&admin), None).await;
    assert_eq!(res.body["navOrder"], json!([]));
    assert_eq!(res.body["hiddenNav"], json!([]));

    let res = app
        .call(
            Method::PATCH,
            "/api/v1/users/me/preferences",
            Some(&admin),
            Some(json!({"navOrder": ["/livetv", " /search", "/livetv"], "hiddenNav": ["/webtv"]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    // Trimmed and without duplicates.
    assert_eq!(res.body["navOrder"], json!(["/livetv", "/search"]));
    assert_eq!(res.body["hiddenNav"], json!(["/webtv"]));
    // Other preferences are untouched; another user has their own menu.
    assert_eq!(res.body["theme"], "dark");
    let res = app.call(Method::GET, "/api/v1/users/me/preferences", Some(&viewer), None).await;
    assert_eq!(res.body["hiddenNav"], json!([]));

    for bad in [json!({"hiddenNav": ["livetv"]}), json!({"navOrder": ["/a b"]})] {
        let res =
            app.call(Method::PATCH, "/api/v1/users/me/preferences", Some(&admin), Some(bad)).await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    }
}
