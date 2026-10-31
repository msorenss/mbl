//! Online discovery permissions and public-address rejection through REST and MCP.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_auth::tokens::{Scope, ScopeSet};
use mbl_core::{Actor, UserId};
use mbl_mcp::{caller::Caller, server::JsonRpcError};
use serde_json::json;

#[tokio::test]
async fn online_routes_require_auth_and_reject_non_public_sources() {
    let (app, db, services) = TestApp::new_with_services().await;
    assert_eq!(
        app.call(Method::GET, "/api/v1/online/search?q=test", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let admin = app.setup_admin().await;
    let disabled = app.call(Method::GET, "/api/v1/online/search?q=test", Some(&admin), None).await;
    assert_eq!(disabled.status, StatusCode::SERVICE_UNAVAILABLE);
    services.plugins.update_config("webdrama", true, json!({})).await.unwrap();
    let profile = app.create_user(&admin, "OnlineProfile").await;
    let id = services.auth.authenticate(&profile).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET is_managed=1,content_rating_ceiling='PG' WHERE id=?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    assert_eq!(
        app.call(Method::GET, "/api/v1/online/search?q=test", Some(&profile), None).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.call(
            Method::POST,
            "/api/v1/online/inspect",
            Some(&profile),
            Some(json!({"url":"https://example.org/show"}))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.call(Method::GET, "/api/v1/online/search?q=x", Some(&admin), None).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    for url in [
        "http://127.0.0.1/",
        "http://169.254.169.254/latest/meta-data",
        "file:///etc/passwd",
        "https://user:pass@example.org/",
    ] {
        let res = app
            .call(Method::POST, "/api/v1/online/inspect", Some(&admin), Some(json!({"url":url})))
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{url}: {}", res.body);
    }
    let old = app.call(Method::GET, "/api/v1/plugins/webdrama/channels", Some(&admin), None).await;
    assert_eq!(
        old.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "legacy demo must not remain advertised as real content"
    );
}

#[tokio::test]
async fn online_mcp_preserves_profile_and_address_errors() {
    let (app, db, services) = TestApp::new_with_services().await;
    app.setup_admin().await;
    services.plugins.update_config("webdrama", true, json!({})).await.unwrap();
    let mut actor = Actor::user(UserId(1));
    actor.is_managed = true;
    let mut caller = Caller {
        actor,
        scopes: ScopeSet::new([Scope::Read]),
        client: "online test".into(),
        token_id: None,
    };
    let denied = mbl_mcp::tools::execute_tool(
        "search_online_sources",
        &json!({"query":"A show"}),
        &caller,
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, JsonRpcError::FORBIDDEN);
    caller.actor.is_managed = false;
    let invalid = mbl_mcp::tools::execute_tool(
        "inspect_online_source",
        &json!({"url":"http://127.0.0.1/private"}),
        &caller,
        &db,
        &services,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(invalid.code, JsonRpcError::INVALID_PARAMS);
}

#[tokio::test]
async fn online_player_routes_preserve_auth_profile_and_private_address_checks() {
    let (app, db, services) = TestApp::new_with_services().await;
    for path in [
        "/api/v1/online/history",
        "/api/v1/online/playback/missing/index.m3u8",
        "/api/v1/online/playback/missing/resource/0",
    ] {
        assert_eq!(app.call(Method::GET, path, None, None).await.status, StatusCode::UNAUTHORIZED);
    }
    let admin = app.setup_admin().await;
    services.plugins.update_config("webdrama", true, json!({})).await.unwrap();
    let response = app
        .call(
            Method::POST,
            "/api/v1/online/playback",
            Some(&admin),
            Some(json!({"url":"http://127.0.0.1/private"})),
        )
        .await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        app.call(Method::GET, "/api/v1/online/history", Some(&admin), None).await.body,
        json!([])
    );
    assert_eq!(
        app.call(Method::DELETE, "/api/v1/online/playback/missing", Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let profile = app.create_user(&admin, "OnlinePlayerProfile").await;
    let id = services.auth.authenticate(&profile).await.unwrap().actor.user_id;
    sqlx::query("UPDATE users SET is_managed=1 WHERE id=?")
        .bind(id.0)
        .execute(&db.write)
        .await
        .unwrap();
    for path in [
        "/api/v1/online/history",
        "/api/v1/online/playback/missing/index.m3u8",
        "/api/v1/online/playback/missing/resource/0",
    ] {
        assert_eq!(
            app.call(Method::GET, path, Some(&profile), None).await.status,
            StatusCode::FORBIDDEN
        );
    }
}
