//! Shared helpers for HTTP-level integration tests.
#![allow(dead_code, clippy::unwrap_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use mbl_server::Config;
use serde_json::{Value, json};
use tower::ServiceExt;

pub struct TestApp {
    pub router: Router,
    pub dir: tempfile::TempDir,
}

pub struct Res {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl TestApp {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.paths.data_dir = dir.path().join("data").to_str().unwrap().to_owned();
        let server = mbl_server::build(config).await.unwrap();
        Self { router: server.router, dir }
    }

    pub async fn send(&self, req: Request<Body>) -> Res {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
        };
        Res { status, headers, body }
    }

    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Res {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        self.send(req).await
    }

    /// Run setup and return the admin token.
    pub async fn setup_admin(&self) -> String {
        let res = self
            .call(
                Method::POST,
                "/api/v1/system/setup",
                None,
                Some(json!({ "username": "admin", "password": "admin-password" })),
            )
            .await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        res.body["token"].as_str().unwrap().to_owned()
    }

    pub async fn login(&self, username: &str, password: &str) -> Res {
        self.call(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "username": username, "password": password, "deviceName": "test" })),
        )
        .await
    }

    /// Create a non-admin user and return their token.
    pub async fn create_user(&self, admin: &str, username: &str) -> String {
        let password = format!("{username}-password");
        let res = self
            .call(
                Method::POST,
                "/api/v1/users",
                Some(admin),
                Some(json!({ "username": username, "password": password })),
            )
            .await;
        assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
        self.login(username, &password).await.body["token"].as_str().unwrap().to_owned()
    }

    /// Create a directory under the test's temp dir and return its path.
    pub fn media_dir(&self, rel: &str) -> String {
        let path = self.dir.path().join("media").join(rel);
        std::fs::create_dir_all(&path).unwrap();
        path.to_str().unwrap().to_owned()
    }
}
