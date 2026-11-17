//! HTTPS certificate administration and the listener that serves the stored certificate.
#![allow(clippy::unwrap_used)]
mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::TestApp;
use mbl_db::repo::settings::SettingsRepo;
use mbl_server::tls::{Timing, run_with};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

const TLS: &str = "/api/v1/system/tls";
const LOCAL_CA: &str = "/api/v1/system/tls/local-ca";
const ACME: &str = "/api/v1/system/tls/acme";
const CA_PEM: &str = "/api/v1/system/tls/ca.pem";
const FAST: Timing = Timing { poll: Duration::from_millis(50), retry: Duration::from_millis(200) };

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

type Tls = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

/// Connect to the listener the way a browser would, trusting only `root_pem`, and finish the
/// handshake. Returns the stream and the certificate the server presented.
fn connect(port: u16, root_pem: &str) -> Result<(Tls, Vec<u8>), String> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut roots = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(root_pem.as_bytes()) {
        roots.add(cert.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = ServerName::try_from("127.0.0.1").map_err(|e| e.to_string())?;
    let mut conn =
        rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    let mut sock = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    while conn.is_handshaking() {
        conn.complete_io(&mut sock).map_err(|e| e.to_string())?;
    }
    let leaf = conn.peer_certificates().and_then(|c| c.first()).ok_or("no certificate")?.to_vec();
    Ok((rustls::StreamOwned::new(conn, sock), leaf))
}

/// Send one raw HTTP/1.1 request on an established connection and return the whole response.
fn exchange(mut stream: Tls, request: &str) -> Result<String, String> {
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    // The server may close without a TLS close_notify; what was read is still the response.
    let _ = stream.read_to_end(&mut response);
    Ok(String::from_utf8_lossy(&response).into_owned())
}

/// Send `GET /health` on an established connection and return the HTTP status.
fn health(stream: Tls) -> Result<u16, String> {
    exchange(stream, "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| "no HTTP status".to_owned())
}

/// One full request; returns the presented certificate when the server answered 200.
async fn fetch(port: u16, root_pem: &str) -> Result<Vec<u8>, String> {
    let root = root_pem.to_owned();
    tokio::task::spawn_blocking(move || {
        let (stream, leaf) = connect(port, &root)?;
        match health(stream)? {
            200 => Ok(leaf),
            status => Err(format!("HTTP {status}")),
        }
    })
    .await
    .unwrap()
}

/// Poll `probe` until it returns a value or ten seconds pass.
async fn eventually<T, F, Fut>(what: &str, mut probe: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    for _ in 0..200 {
        if let Some(value) = probe().await {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

struct Fixture {
    app: TestApp,
    db: mbl_db::Db,
    admin: String,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    /// An app with a local certificate for 127.0.0.1 and HTTPS enabled on `port`.
    async fn serving(port: u16) -> Self {
        let (app, db, _services) = TestApp::new_with_services().await;
        let admin = app.setup_admin().await;
        let issued = app
            .call(Method::POST, LOCAL_CA, Some(&admin), Some(json!({ "hostnames": ["127.0.0.1"] })))
            .await;
        assert_eq!(issued.status, StatusCode::OK, "{:?}", issued.body);
        let enabled = app
            .call(Method::PATCH, TLS, Some(&admin), Some(json!({ "enabled": true, "port": port })))
            .await;
        assert_eq!(enabled.status, StatusCode::OK, "{:?}", enabled.body);
        let stop = CancellationToken::new();
        let task = tokio::spawn(run_with(
            db.clone(),
            app.router.clone(),
            "127.0.0.1".to_owned(),
            stop.clone(),
            FAST,
        ));
        Self { app, db, admin, stop, task }
    }

    async fn status(&self) -> Value {
        let res = self.app.call(Method::GET, TLS, Some(&self.admin), None).await;
        assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
        res.body
    }

    async fn root(&self) -> String {
        let res = self.app.call(Method::GET, CA_PEM, Some(&self.admin), None).await;
        assert_eq!(res.status, StatusCode::OK);
        res.body.as_str().unwrap().to_owned()
    }

    async fn listener_error(&self) -> Option<String> {
        self.status().await["listenerError"].as_str().map(str::to_owned)
    }

    async fn shutdown(self) {
        self.stop.cancel();
        self.task.await.unwrap();
    }
}

#[tokio::test]
async fn tls_administration_is_admin_only() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let member = app.create_user(&admin, "member").await;
    let calls = [
        (Method::GET, TLS, None),
        (Method::PATCH, TLS, Some(json!({ "enabled": false }))),
        (Method::POST, LOCAL_CA, Some(json!({ "hostnames": ["mbl.lan"] }))),
        (Method::POST, "/api/v1/system/tls/tailscale", None),
        (Method::POST, ACME, Some(json!({ "domain": "mbl.example.com", "acceptTerms": true }))),
        (Method::GET, CA_PEM, None),
    ];
    for (method, path, body) in calls {
        let anonymous = app.call(method.clone(), path, None, body.clone()).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        let forbidden = app.call(method.clone(), path, Some(&member), body).await;
        assert_eq!(forbidden.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
    // Nothing above may have created key material or queued a request.
    let status = app.call(Method::GET, TLS, Some(&admin), None).await.body;
    assert_eq!(status["hasCertificate"], json!(false));
    assert_eq!(status["caAvailable"], json!(false));
    assert_eq!(status["jobState"], json!("idle"));
    assert_eq!(
        app.call(Method::GET, CA_PEM, Some(&admin), None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn listener_settings_are_validated_as_a_whole() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let patch = |body: Value| app.call(Method::PATCH, TLS, Some(&admin), Some(body));

    for port in [80, 1023, 8081] {
        let res = patch(json!({ "port": port })).await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "port {port}");
    }
    // Enabling without a certificate is refused, and the port sent with it is not kept.
    let res = patch(json!({ "enabled": true, "port": 9443 })).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let status = app.call(Method::GET, TLS, Some(&admin), None).await.body;
    assert_eq!(status["port"], json!(8443));
    assert_eq!(status["enabled"], json!(false));

    let res = patch(json!({ "port": 9443 })).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["port"], json!(9443));

    for hostnames in
        [json!([]), json!(["*"]), json!([".."]), json!(["a b"]), json!(["ok.lan", "a..b"])]
    {
        let res = app
            .call(Method::POST, LOCAL_CA, Some(&admin), Some(json!({ "hostnames": hostnames })))
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{hostnames}");
    }
    assert_eq!(
        app.call(Method::GET, TLS, Some(&admin), None).await.body["caAvailable"],
        json!(false)
    );
}

#[tokio::test]
async fn imported_certificates_are_checked_and_report_their_real_expiry() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let ca = mbl_remote::tls::generate_local_ca().unwrap();
    let pair =
        mbl_remote::tls::issue_server_cert(&ca.key_pem, &["mbl.example.com".into()]).unwrap();
    let import = |domain: &str, cert: &str, key: &str| {
        app.call(
            Method::POST,
            ACME,
            Some(&admin),
            Some(json!({ "domain": domain, "certPem": cert, "keyPem": key })),
        )
    };

    let rejected = [
        import("mbl.example.com", &pair.cert_pem, "").await,
        import("mbl.example.com", &pair.cert_pem, &ca.key_pem).await,
        import("mbl.example.com", "not a certificate", &pair.key_pem).await,
        import("other.example.com", &pair.cert_pem, &pair.key_pem).await,
        import("..", &pair.cert_pem, &pair.key_pem).await,
        import("server.local", &pair.cert_pem, &pair.key_pem).await,
        import("192.168.1.10", &pair.cert_pem, &pair.key_pem).await,
    ];
    for res in rejected {
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{:?}", res.body);
    }
    let unchanged = app.call(Method::GET, TLS, Some(&admin), None).await.body;
    assert_eq!(unchanged["hasCertificate"], json!(false));
    // Automatic issuance needs the subscriber agreement to be accepted.
    let res = app
        .call(Method::POST, ACME, Some(&admin), Some(json!({ "domain": "mbl.example.com" })))
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    let res = import("MBL.Example.com.", &pair.cert_pem, &pair.key_pem).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    assert_eq!(res.body["mode"], json!("letsencrypt"));
    assert_eq!(res.body["hostnames"], json!(["mbl.example.com"]));
    assert_eq!(res.body["automaticRenewal"], json!(false));
    // The expiry is read from the certificate (one year here), not assumed to be 90 days.
    let expires = res.body["expiresAt"].as_i64().unwrap();
    assert!((expires - now() - 365 * 86400).abs() < 2 * 86400, "{expires}");
}

#[tokio::test]
async fn certificate_swaps_never_take_down_a_working_listener() {
    let port = free_port();
    let fx = Fixture::serving(port).await;
    let root = fx.root().await;
    let first = eventually("HTTPS to answer", || async { fetch(port, &root).await.ok() }).await;
    assert_eq!(fx.listener_error().await, None);
    let expires = fx.status().await["expiresAt"].as_i64().unwrap();
    assert!((expires - now() - 365 * 86400).abs() < 2 * 86400);

    // A connection established before the swap keeps working after it.
    let held_root = root.clone();
    let (held, held_leaf) =
        tokio::task::spawn_blocking(move || connect(port, &held_root)).await.unwrap().unwrap();
    assert_eq!(held_leaf, first);

    // Reissue: new connections get the new certificate, from the same root.
    let res = fx
        .app
        .call(
            Method::POST,
            LOCAL_CA,
            Some(&fx.admin),
            Some(json!({ "hostnames": ["127.0.0.1", "mbl.lan"] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(fx.root().await, root);
    let second = eventually("the reissued certificate", || async {
        fetch(port, &root).await.ok().filter(|leaf| *leaf != first)
    })
    .await;
    assert_eq!(tokio::task::spawn_blocking(move || health(held)).await.unwrap(), Ok(200));

    // A key that does not belong to the certificate is refused and the old pair keeps serving.
    let repo = SettingsRepo::new(fx.db.clone());
    let good_cert = repo.get("tls_cert_pem").await.unwrap().unwrap();
    let good_key = repo.get("tls_key_pem").await.unwrap().unwrap();
    let stranger = mbl_remote::tls::generate_local_ca().unwrap();
    repo.set("tls_key_pem", &stranger.key_pem).await.unwrap();
    let error = eventually("the listener error", || fx.listener_error()).await;
    assert!(error.contains("not usable"), "{error}");
    assert_eq!(fetch(port, &root).await.unwrap(), second);

    // So is text that is not a certificate at all.
    repo.set_many(&[("tls_cert_pem", "garbage"), ("tls_key_pem", "garbage")]).await.unwrap();
    tokio::time::sleep(FAST.poll * 6).await;
    assert!(fx.listener_error().await.is_some());
    assert_eq!(fetch(port, &root).await.unwrap(), second);

    // Repairing the settings clears the error.
    repo.set_many(&[("tls_cert_pem", &good_cert), ("tls_key_pem", &good_key)]).await.unwrap();
    eventually("the error to clear", || async {
        fx.listener_error().await.is_none().then_some(())
    })
    .await;
    assert_eq!(fetch(port, &root).await.unwrap(), second);

    // Switching HTTPS off closes the port.
    let res =
        fx.app.call(Method::PATCH, TLS, Some(&fx.admin), Some(json!({ "enabled": false }))).await;
    assert_eq!(res.status, StatusCode::OK);
    eventually("the port to close", || async {
        TcpStream::connect(("127.0.0.1", port)).is_err().then_some(())
    })
    .await;
    fx.shutdown().await;
}

#[tokio::test]
async fn a_busy_port_is_reported_and_retried_while_the_old_port_keeps_serving() {
    let (old_port, new_port) = (free_port(), free_port());
    let fx = Fixture::serving(old_port).await;
    let root = fx.root().await;
    let leaf = eventually("HTTPS to answer", || async { fetch(old_port, &root).await.ok() }).await;

    let squatter = std::net::TcpListener::bind(("127.0.0.1", new_port)).unwrap();
    let res =
        fx.app.call(Method::PATCH, TLS, Some(&fx.admin), Some(json!({ "port": new_port }))).await;
    assert_eq!(res.status, StatusCode::OK);
    let error = eventually("the listener error", || fx.listener_error()).await;
    assert!(error.contains("cannot listen") && error.contains(&new_port.to_string()), "{error}");
    assert_eq!(fetch(old_port, &root).await.unwrap(), leaf);

    // Once the port is free the listener moves by itself and the error goes away.
    drop(squatter);
    eventually("HTTPS on the new port", || async { fetch(new_port, &root).await.ok() }).await;
    eventually("the error to clear", || async {
        fx.listener_error().await.is_none().then_some(())
    })
    .await;
    eventually("the old port to close", || async {
        TcpStream::connect(("127.0.0.1", old_port)).is_err().then_some(())
    })
    .await;
    fx.shutdown().await;
}

#[tokio::test]
async fn unreadable_settings_do_not_stop_the_listener() {
    let port = free_port();
    let fx = Fixture::serving(port).await;
    let root = fx.root().await;
    eventually("HTTPS to answer", || async { fetch(port, &root).await.ok() }).await;

    // Every settings read now fails. The listener must treat that as "unknown", not "off".
    fx.db.read.close().await;
    tokio::time::sleep(FAST.poll * 10).await;
    let handshake_root = root.clone();
    let handshake =
        tokio::task::spawn_blocking(move || connect(port, &handshake_root).map(|_| ())).await;
    assert_eq!(handshake.unwrap(), Ok(()));
    fx.shutdown().await;
}

#[tokio::test]
async fn session_cookies_are_secure_exactly_when_the_login_came_over_https() {
    let port = free_port();
    let fx = Fixture::serving(port).await;
    let root = fx.root().await;
    eventually("HTTPS to answer", || async { fetch(port, &root).await.ok() }).await;

    let body = json!({ "username": "admin", "password": "admin-password", "deviceName": "test" })
        .to_string();
    let request = format!(
        "POST /api/v1/auth/login HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let response = tokio::task::spawn_blocking(move || {
        let (stream, _) = connect(port, &root)?;
        exchange(stream, &request)
    })
    .await
    .unwrap()
    .unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let cookie = response
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .expect("session cookie");
    assert!(cookie.contains("; Secure") && cookie.contains("HttpOnly"), "{cookie}");

    // The same login over the plain HTTP listener must stay usable there.
    let plain = fx.app.login("admin", "admin-password").await;
    assert_eq!(plain.status, StatusCode::OK);
    let cookie = plain.headers["set-cookie"].to_str().unwrap();
    assert!(!cookie.contains("Secure") && cookie.contains("HttpOnly"), "{cookie}");
    fx.shutdown().await;
}
