#![allow(clippy::unwrap_used)]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn setup_runs_exactly_once() {
    let app = TestApp::new().await;
    let status = app.call(Method::GET, "/api/v1/system/setup", None, None).await;
    assert_eq!(status.body, json!({ "needsSetup": true }));

    let token = app.setup_admin().await;
    let me = app.call(Method::GET, "/api/v1/auth/me", Some(&token), None).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["username"], "admin");
    assert_eq!(me.body["isAdmin"], true);

    let again = app
        .call(
            Method::POST,
            "/api/v1/system/setup",
            None,
            Some(json!({ "username": "evil", "password": "evil-password" })),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    let status = app.call(Method::GET, "/api/v1/system/setup", None, None).await;
    assert_eq!(status.body, json!({ "needsSetup": false }));
}

#[tokio::test]
async fn setup_validates_input() {
    let app = TestApp::new().await;
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/setup",
            None,
            Some(json!({ "username": "admin", "password": "short" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.body["error"]["code"], "validation");
}

#[tokio::test]
async fn login_logout_and_bad_password() {
    let app = TestApp::new().await;
    app.setup_admin().await;

    let bad = app.login("admin", "wrong-password").await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    let unknown = app.login("nobody", "whatever-password").await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);

    let ok = app.login("ADMIN", "admin-password").await;
    assert_eq!(ok.status, StatusCode::OK);
    let cookie = ok.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cookie.starts_with("mbl_session="));
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    let token = ok.body["token"].as_str().unwrap().to_owned();

    let out = app.call(Method::POST, "/api/v1/auth/logout", Some(&token), None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let me = app.call(Method::GET, "/api/v1/auth/me", Some(&token), None).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let app = TestApp::new().await;
    let res = app.call(Method::GET, "/api/v1/auth/me", None, None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    let res = app.call(Method::GET, "/api/v1/auth/me", Some("garbage"), None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_is_rate_limited() {
    let app = TestApp::new().await;
    app.setup_admin().await;
    for _ in 0..5 {
        assert_eq!(app.login("admin", "wrong-password").await.status, StatusCode::UNAUTHORIZED);
    }
    let limited = app.login("admin", "admin-password").await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers.contains_key(header::RETRY_AFTER));
    assert_eq!(limited.body["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn cookie_auth_and_cross_origin_protection() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    let cookie = format!("mbl_session={token}");

    let me = app
        .send(
            Request::get("/api/v1/auth/me")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(me.status, StatusCode::OK);

    let cross_site = app
        .send(
            Request::post("/api/v1/auth/logout")
                .header(header::COOKIE, &cookie)
                .header(header::HOST, "mbl.local:8420")
                .header(header::ORIGIN, "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(cross_site.status, StatusCode::FORBIDDEN);

    let same_site = app
        .send(
            Request::post("/api/v1/auth/logout")
                .header(header::COOKIE, &cookie)
                .header(header::HOST, "mbl.local:8420")
                .header(header::ORIGIN, "http://mbl.local:8420")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(same_site.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn user_management_requires_admin() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;

    let created = app
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({ "username": "anna", "password": "anna-password", "displayName": "Anna" })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["isAdmin"], false);
    let anna_id = created.body["id"].as_i64().unwrap();

    let dup = app
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({ "username": "Anna", "password": "other-password" })),
        )
        .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let anna = app.login("anna", "anna-password").await.body["token"].as_str().unwrap().to_owned();
    assert_eq!(
        app.call(Method::GET, "/api/v1/users", Some(&anna), None).await.status,
        StatusCode::FORBIDDEN
    );
    let own = app.call(Method::GET, &format!("/api/v1/users/{anna_id}"), Some(&anna), None).await;
    assert_eq!(own.status, StatusCode::OK);
    let other = app.call(Method::GET, "/api/v1/users/1", Some(&anna), None).await;
    assert_eq!(other.status, StatusCode::NOT_FOUND);

    let escalate = app
        .call(
            Method::PATCH,
            &format!("/api/v1/users/{anna_id}"),
            Some(&anna),
            Some(json!({ "isAdmin": true })),
        )
        .await;
    assert_eq!(escalate.status, StatusCode::FORBIDDEN);
    let rename = app
        .call(
            Method::PATCH,
            &format!("/api/v1/users/{anna_id}"),
            Some(&anna),
            Some(json!({ "displayName": "Anna B" })),
        )
        .await;
    assert_eq!(rename.body["displayName"], "Anna B");

    let list = app.call(Method::GET, "/api/v1/users", Some(&admin), None).await;
    assert_eq!(list.body.as_array().unwrap().len(), 2);

    let del =
        app.call(Method::DELETE, &format!("/api/v1/users/{anna_id}"), Some(&admin), None).await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let gone = app.call(Method::GET, "/api/v1/auth/me", Some(&anna), None).await;
    assert_eq!(gone.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn last_admin_is_protected() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let demote = app
        .call(Method::PATCH, "/api/v1/users/1", Some(&admin), Some(json!({ "isAdmin": false })))
        .await;
    assert_eq!(demote.status, StatusCode::CONFLICT);
    let delete_self = app.call(Method::DELETE, "/api/v1/users/1", Some(&admin), None).await;
    assert_eq!(delete_self.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn sessions_can_be_listed_and_revoked() {
    let app = TestApp::new().await;
    let first = app.setup_admin().await;
    let second =
        app.login("admin", "admin-password").await.body["token"].as_str().unwrap().to_owned();

    let list = app.call(Method::GET, "/api/v1/auth/sessions", Some(&first), None).await;
    let sessions = list.body.as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    let other = sessions.iter().find(|s| s["current"] == false).unwrap();
    let other_id = other["id"].as_i64().unwrap();

    let revoke = app
        .call(Method::DELETE, &format!("/api/v1/auth/sessions/{other_id}"), Some(&first), None)
        .await;
    assert_eq!(revoke.status, StatusCode::NO_CONTENT);
    assert_eq!(
        app.call(Method::GET, "/api/v1/auth/me", Some(&second), None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.call(Method::GET, "/api/v1/auth/me", Some(&first), None).await.status,
        StatusCode::OK
    );
}
