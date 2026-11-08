#![allow(clippy::unwrap_used)]

use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::process::Command;

async fn wait_for_server(client: &Client, base: &str, previous: Option<i64>) -> Value {
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            if let Ok(response) = client.get(format!("{base}/api/v1/system/info")).send().await
                && response.status().is_success()
                && let Ok(info) = response.json::<Value>().await
                && let Some(started) = info["startedAt"].as_i64()
                && Some(started) != previous
            {
                return info;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn admin_can_restart_the_server_on_the_same_port_without_losing_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_mbl-server"))
        .args(["--data-dir", dir.path().to_str().unwrap(), "--port", &port.to_string()])
        .env("MBL_SERVER__BIND", "127.0.0.1")
        .env("MBL_SERVER__MDNS", "false")
        .env("MBL_SCANNER__WATCH", "false")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let client = Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let before = wait_for_server(&client, &base, None).await;
    let restart = format!("{base}/api/v1/system/restart");
    assert_eq!(client.post(&restart).send().await.unwrap().status(), StatusCode::UNAUTHORIZED);

    let setup: Value = client
        .post(format!("{base}/api/v1/system/setup"))
        .json(&json!({"username": "admin", "password": "admin-test-password"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = setup["token"].as_str().unwrap();
    let accepted = client.post(&restart).bearer_auth(token).send().await.unwrap();
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let receipt: Value = accepted.json().await.unwrap();
    assert_eq!(receipt["previousStartedAt"], before["startedAt"]);

    let after = wait_for_server(&client, &base, before["startedAt"].as_i64()).await;
    assert_eq!(after["serverId"], before["serverId"]);
    let me = client.get(format!("{base}/api/v1/auth/me")).bearer_auth(token).send().await.unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    child.kill().await.unwrap();
}
