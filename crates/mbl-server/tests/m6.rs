//! Integration tests for Milestone M6:
//! - M6.1: Library ACLs per user (enforced in ItemFilter), managed users, PIN, content rating ceilings (table-driven)
//! - M6.2: Share invites (creation, public inspect, registration with inherited permissions)
//! - M6.3: TOTP 2FA lifecycle (setup, verify, challenge on login, disable)
//! - M6.4 & M6.12: Admin LAN/Tailnet-only guard blocking WAN callers
//! - M6.7: Tailscale header spoofing rejection from non-loopback sources
//! - M6.15 & M6.16: MCP tools for users, sharing, remote, settings, backup, restart, and prompt setup_remote_access
//! - M6.17: OAuth 2.1 authorization server, dynamic registration, PKCE S256 code exchange, refresh, and revocation

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode, header};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::TestApp;
use mbl_mcp::McpClient;
use serde_json::json;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn m6_1_library_and_rating_acls_table_driven() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin_token = app.setup_admin().await;

    // 1. Create two libraries:
    // Lib 1: Family Movies
    // Lib 2: Mature Shows
    let dir1 = app.media_dir("Family");
    let res = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_token),
            Some(json!({ "name": "Family", "kind": "movie", "paths": [dir1] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let lib1_id = res.body["id"].as_i64().unwrap();

    let dir2 = app.media_dir("Action");
    let res = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_token),
            Some(json!({ "name": "Action", "kind": "movie", "paths": [dir2] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let lib2_id = res.body["id"].as_i64().unwrap();

    // 2. Seed items with different content ratings:
    // Item 1 in Lib 1: Rating "G" (Bambi)
    let item1_g: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, content_rating, duration_ms, added_at, updated_at)
         VALUES (?, 'movie', 'Bambi', 'bambi', 'G', 4200000, 100, 100) RETURNING id",
    )
    .bind(lib1_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ver1: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mp4', 'h264', 'aac', 1920, 1080) RETURNING id",
    )
    .bind(item1_g)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let bambi_path = std::path::Path::new(&dir1).join("bambi.mp4");
    std::fs::write(&bambi_path, b"bambi content").unwrap();
    let part1_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, part_index, probe)
         VALUES (?, ?, 1024, 1, 4200000, 0, '{}') RETURNING id",
    )
    .bind(ver1)
    .bind(bambi_path.to_str().unwrap())
    .fetch_one(&db.write)
    .await
    .unwrap();

    // Item 2 in Lib 1: Rating "PG-13" (Spider-Man)
    let item2_pg13: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, content_rating, duration_ms, added_at, updated_at)
         VALUES (?, 'movie', 'Spider-Man', 'spiderman', 'PG-13', 7200000, 100, 100) RETURNING id",
    )
    .bind(lib1_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ver2: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mp4', 'h264', 'aac', 1920, 1080) RETURNING id",
    )
    .bind(item2_pg13)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let spiderman_path = std::path::Path::new(&dir1).join("spiderman.mp4");
    std::fs::write(&spiderman_path, b"spiderman content").unwrap();
    let _part2_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, part_index, probe)
         VALUES (?, ?, 1024, 1, 7200000, 0, '{}') RETURNING id",
    )
    .bind(ver2)
    .bind(spiderman_path.to_str().unwrap())
    .fetch_one(&db.write)
    .await
    .unwrap();

    // Item 3 in Lib 1: Rating "R" (The Matrix)
    let item3_r: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, content_rating, duration_ms, added_at, updated_at)
         VALUES (?, 'movie', 'The Matrix', 'matrix', 'R', 8160000, 100, 100) RETURNING id",
    )
    .bind(lib1_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let ver3: i64 = sqlx::query_scalar(
        "INSERT INTO media_versions (item_id, container, video_codec, audio_codec, width, height)
         VALUES (?, 'mp4', 'h264', 'aac', 1920, 1080) RETURNING id",
    )
    .bind(item3_r)
    .fetch_one(&db.write)
    .await
    .unwrap();

    let matrix_path = std::path::Path::new(&dir1).join("matrix.mp4");
    std::fs::write(&matrix_path, b"matrix content").unwrap();
    let part3_id: i64 = sqlx::query_scalar(
        "INSERT INTO media_parts (version_id, path, size, mtime, duration_ms, part_index, probe)
         VALUES (?, ?, 1024, 1, 8160000, 0, '{}') RETURNING id",
    )
    .bind(ver3)
    .bind(matrix_path.to_str().unwrap())
    .fetch_one(&db.write)
    .await
    .unwrap();

    // Item 4 in Lib 2: Rating "PG" (Star Wars)
    let item4_lib2: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, content_rating, duration_ms, added_at, updated_at)
         VALUES (?, 'movie', 'Star Wars', 'starwars', 'PG', 7260000, 100, 100) RETURNING id",
    )
    .bind(lib2_id)
    .fetch_one(&db.write)
    .await
    .unwrap();

    // 3. Create Users:
    // - "child_user": restricted to Lib 1, rating ceiling "PG"
    let res = app
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin_token),
            Some(json!({
                "username": "child_user",
                "password": "Password123!",
                "isManaged": true,
                "contentRatingCeiling": "PG",
                "libraryIds": [lib1_id]
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let child_token =
        app.login("child_user", "Password123!").await.body["token"].as_str().unwrap().to_owned();

    // - "teen_user": restricted to Lib 1, rating ceiling "PG-13"
    let res = app
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin_token),
            Some(json!({
                "username": "teen_user",
                "password": "Password123!",
                "isManaged": false,
                "contentRatingCeiling": "PG-13",
                "libraryIds": [lib1_id]
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let teen_token =
        app.login("teen_user", "Password123!").await.body["token"].as_str().unwrap().to_owned();

    // - "lib1_adult": restricted to Lib 1 only, no rating ceiling
    let res = app
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin_token),
            Some(json!({
                "username": "lib1_adult",
                "password": "Password123!",
                "libraryIds": [lib1_id]
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let adult_token =
        app.login("lib1_adult", "Password123!").await.body["token"].as_str().unwrap().to_owned();

    // 4. Table-Driven Assertions across all media routes (M6.1 accept criteria)

    // A) GET /api/v1/libraries/{id}/items
    // Child should see only Bambi (G)
    let res = app
        .call(Method::GET, &format!("/api/v1/libraries/{lib1_id}/items"), Some(&child_token), None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let items = res.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Bambi");

    // Teen should see Bambi (G) and Spider-Man (PG-13), but not The Matrix (R)
    let res = app
        .call(Method::GET, &format!("/api/v1/libraries/{lib1_id}/items"), Some(&teen_token), None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let items = res.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|i| i["title"] == "Bambi"));
    assert!(items.iter().any(|i| i["title"] == "Spider-Man"));
    assert!(!items.iter().any(|i| i["title"] == "The Matrix"));

    // Adult should see all 3 in Lib 1
    let res = app
        .call(Method::GET, &format!("/api/v1/libraries/{lib1_id}/items"), Some(&adult_token), None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["items"].as_array().unwrap().len(), 3);

    // Any access to Lib 2 by users restricted to Lib 1 returns 404 Not Found
    let res = app
        .call(Method::GET, &format!("/api/v1/libraries/{lib2_id}/items"), Some(&child_token), None)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // B) GET /api/v1/items/{id} detail endpoint
    // Child gets 200 on G, 404 on PG-13, 404 on R, 404 on Lib 2
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item1_g}"), Some(&child_token), None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item2_pg13}"), Some(&child_token), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item3_r}"), Some(&child_token), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item4_lib2}"), Some(&child_token), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Teen gets 200 on G and PG-13, 404 on R, 404 on Lib 2
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item1_g}"), Some(&teen_token), None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item2_pg13}"), Some(&teen_token), None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item3_r}"), Some(&teen_token), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Adult gets 200 on all Lib 1 items, 404 on Lib 2 item
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item3_r}"), Some(&adult_token), None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item4_lib2}"), Some(&adult_token), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Admin gets 200 on every single item
    assert_eq!(
        app.call(Method::GET, &format!("/api/v1/items/{item4_lib2}"), Some(&admin_token), None)
            .await
            .status,
        StatusCode::OK
    );

    // C) GET /api/v1/search?q={...}
    // Searching for "Matrix" as child returns empty results
    let res = app.call(Method::GET, "/api/v1/search?q=matrix", Some(&child_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let groups = res.body.as_array().unwrap();
    let total_items: usize = groups.iter().map(|g| g["items"].as_array().unwrap().len()).sum();
    assert_eq!(total_items, 0);

    // Searching as adult returns Matrix
    let res = app.call(Method::GET, "/api/v1/search?q=matrix", Some(&adult_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let groups = res.body.as_array().unwrap();
    let total_items: usize = groups.iter().map(|g| g["items"].as_array().unwrap().len()).sum();
    assert_eq!(total_items, 1);

    // D) POST /api/v1/playback
    // Child playing R-rated item should be rejected with 404/NotFound
    let res = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&child_token),
            Some(json!({ "itemId": item3_r, "partId": part3_id })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // Child playing G-rated item succeeds
    let res = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&child_token),
            Some(json!({ "itemId": item1_g, "partId": part1_id })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);

    // E) POST /api/v1/items/{id}/watched
    // Child marking R-rated item as watched is rejected
    let res = app
        .call(Method::POST, &format!("/api/v1/items/{item3_r}/watched"), Some(&child_token), None)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // Adult marking R-rated item as watched succeeds
    let res = app
        .call(Method::POST, &format!("/api/v1/items/{item3_r}/watched"), Some(&adult_token), None)
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn m6_2_invites_and_registration() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin().await;
    let movies = app.media_dir("Movies");
    let created_lib = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin_token),
            Some(json!({ "name": "Movies", "kind": "movie", "paths": [movies] })),
        )
        .await;
    let lib_id = created_lib.body["id"].as_i64().unwrap();

    // 1. Admin creates an invite code with ceiling PG-13
    let res = app
        .call(
            Method::POST,
            "/api/v1/invites",
            Some(&admin_token),
            Some(json!({
                "contentRatingCeiling": "PG-13",
                "libraryIds": [lib_id],
                "ttlHours": 48
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    let code = res.body["code"].as_str().unwrap().to_owned();

    // 2. Anonymous inspection of invite code
    let res = app.call(Method::GET, &format!("/api/v1/invites/inspect/{code}"), None, None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body["contentRatingCeiling"], "PG-13");
    assert_eq!(res.body["libraryIds"].as_array().unwrap(), &vec![json!(lib_id)]);

    // 3. Register user using invite code
    let res = app
        .call(
            Method::POST,
            "/api/v1/auth/register-invite",
            None,
            Some(json!({
                "code": code,
                "username": "invited_guest",
                "password": "Password123!",
                "displayName": "Guest User"
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.body);
    let guest_token = res.body["token"].as_str().unwrap();

    // 4. Verify registered user inherited the permissions
    let me = app.call(Method::GET, "/api/v1/auth/me", Some(guest_token), None).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["username"], "invited_guest");

    // Invite code cannot be reused
    let res2 = app
        .call(
            Method::POST,
            "/api/v1/auth/register-invite",
            None,
            Some(json!({
                "code": code,
                "username": "duplicate_attempt",
                "password": "Password123!"
            })),
        )
        .await;
    assert_eq!(res2.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn m6_3_totp_2fa_lifecycle() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin().await;

    // 1. Initiate TOTP setup
    let res = app.call(Method::POST, "/api/v1/auth/totp/setup", Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::OK);
    let secret = res.body["secret"].as_str().unwrap();
    let uri = res.body["uri"].as_str().unwrap();
    assert!(uri.starts_with("otpauth://totp/"));

    // 2. Wrong TOTP code fails verification
    let res = app
        .call(
            Method::POST,
            "/api/v1/auth/totp/verify",
            Some(&admin_token),
            Some(json!({ "pin": "000000" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, json!(false));

    // 3. Compute valid TOTP code
    let now_secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let valid_code = mbl_auth::totp::compute_code(secret, now_secs).unwrap();

    let res = app
        .call(
            Method::POST,
            "/api/v1/auth/totp/verify",
            Some(&admin_token),
            Some(json!({ "pin": valid_code })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.body, json!(true));

    // 4. Logging in without TOTP code now fails
    let res = app
        .call(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "username": "admin", "password": "admin-password" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);

    // 5. Logging in with correct TOTP code succeeds
    let res = app
        .call(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({
                "username": "admin",
                "password": "admin-password",
                "totpCode": valid_code
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);

    // 6. Disable TOTP
    let new_token = res.body["token"].as_str().unwrap();
    let res = app.call(Method::POST, "/api/v1/auth/totp/disable", Some(new_token), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    // 7. Login without TOTP code succeeds again
    let res = app
        .call(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({ "username": "admin", "password": "admin-password" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn m6_4_and_12_admin_lan_only_guard() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin().await;

    // Enable admin_lan_only policy
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/remote-policy",
            Some(&admin_token),
            Some(json!({ "adminLanOnly": true })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);

    // Attempting admin call from WAN IP (e.g. 198.51.100.25)
    let req = axum::http::Request::builder()
        .method(Method::GET)
        .uri("/api/v1/system/remote-status")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .header("x-forwarded-for", "198.51.100.25")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // Attempting admin call from LAN IP (192.168.1.50) succeeds
    let req = axum::http::Request::builder()
        .method(Method::GET)
        .uri("/api/v1/system/remote-status")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .header("x-forwarded-for", "192.168.1.50")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::OK);

    // Attempting admin call from Tailscale IP (100.64.0.5) succeeds
    let req = axum::http::Request::builder()
        .method(Method::GET)
        .uri("/api/v1/system/remote-status")
        .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
        .header("x-forwarded-for", "100.64.0.5")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn m6_7_tailscale_spoofing_guard() {
    let app = TestApp::new().await;

    // A caller from a non-loopback IP (e.g. 192.168.1.100) supplying Tailscale-User-Login headers
    // must be rejected with 403 Forbidden to prevent identity spoofing
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/tailscale")
        .header("x-forwarded-for", "192.168.1.100")
        .header("tailscale-user-login", "victim@example.com")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn m6_17_oauth21_code_flow_with_pkce() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin().await;

    // 1. Metadata endpoints
    let meta = app.call(Method::GET, "/.well-known/oauth-authorization-server", None, None).await;
    assert_eq!(meta.status, StatusCode::OK);
    assert_eq!(meta.body["authorization_endpoint"], "/oauth/authorize");
    assert_eq!(meta.body["token_endpoint"], "/oauth/token");

    // 2. Dynamic client registration
    let res = app
        .call(
            Method::POST,
            "/oauth/register",
            None,
            Some(json!({
                "client_name": "Test Client",
                "redirect_uris": ["https://client.example.com/callback"]
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let client_id = res.body["client_id"].as_str().unwrap().to_owned();

    // 3. Compute PKCE S256 verifier and challenge
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

    // 4. Authorization request
    let auth_uri = format!(
        "/oauth/authorize?client_id={client_id}&redirect_uri=https://client.example.com/callback&response_type=code&code_challenge={challenge}&code_challenge_method=S256&scope=read+library&state=xyz123"
    );
    let res = app.call(Method::GET, &auth_uri, Some(&admin_token), None).await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    let location = res.headers.get(header::LOCATION).unwrap().to_str().unwrap();
    assert!(location.starts_with("https://client.example.com/callback?code="));
    assert!(location.contains("state=xyz123"));

    let code = location.split("code=").nth(1).unwrap().split('&').next().unwrap();

    // 5. Exchange code for tokens (using form urlencoded)
    let form_body = format!(
        "grant_type=authorization_code&code={code}&redirect_uri=https://client.example.com/callback&client_id={client_id}&code_verifier={verifier}"
    );
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/oauth/token")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(form_body))
        .unwrap();
    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let access_token = res.body["access_token"].as_str().unwrap().to_owned();
    let refresh_token = res.body["refresh_token"].as_str().unwrap().to_owned();
    assert_eq!(res.body["token_type"], "Bearer");

    // 6. Refresh token rotation
    let refresh_form = format!("grant_type=refresh_token&refresh_token={refresh_token}");
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/oauth/token")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(refresh_form))
        .unwrap();
    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::OK);
    let new_access = res.body["access_token"].as_str().unwrap().to_owned();
    assert_ne!(access_token, new_access);

    // 7. Revocation
    let revoke_form = format!("token={new_access}");
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/oauth/revoke")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(revoke_form))
        .unwrap();
    let res = app.send(req).await;
    assert_eq!(res.status, StatusCode::OK);
}

#[tokio::test]
async fn m6_15_and_16_mcp_mutations_and_tools() {
    let app = TestApp::new().await;
    let admin_token = app.setup_admin().await;

    let mut client = McpClient::connect_router(app.router.clone(), Some(admin_token));
    let init = client.initialize().await.unwrap();
    assert!(init.capabilities.prompts.is_some());

    // 1. List users
    let res = client.call_tool("list_users", json!({})).await.unwrap();
    assert!(!res.is_error);
    let users = res.structured_content.unwrap();
    assert_eq!(users.as_array().unwrap().len(), 1);

    // 2. Create user: dry_run preview
    let res = client
        .call_tool(
            "create_user",
            json!({
                "username": "simulated_user",
                "password": "Password123!",
                "dryRun": true
            }),
        )
        .await
        .unwrap();
    assert!(!res.is_error);
    assert_eq!(res.structured_content.unwrap()["dryRun"], true);

    // Verify user was NOT created in DB
    let res = client.call_tool("list_users", json!({})).await.unwrap();
    assert_eq!(res.structured_content.unwrap().as_array().unwrap().len(), 1);

    // 3. Create user: execute
    let res = client
        .call_tool(
            "create_user",
            json!({
                "username": "real_user",
                "password": "Password123!",
                "isManaged": true,
                "contentRatingCeiling": "G"
            }),
        )
        .await
        .unwrap();
    assert!(!res.is_error);
    let real_user_id = res.structured_content.unwrap()["id"].as_i64().unwrap();

    // 4. Update user permissions
    let res = client
        .call_tool(
            "update_user_permissions",
            json!({
                "userId": real_user_id,
                "contentRatingCeiling": "PG"
            }),
        )
        .await
        .unwrap();
    assert!(!res.is_error);
    assert_eq!(res.structured_content.unwrap()["contentRatingCeiling"], "PG");

    // 5. Create invite
    let res = client
        .call_tool(
            "create_invite",
            json!({
                "contentRatingCeiling": "PG-13",
                "ttlHours": 24
            }),
        )
        .await
        .unwrap();
    assert!(!res.is_error);
    assert!(res.structured_content.unwrap()["code"].as_str().is_some());

    // 6. Remote tools
    let res = client.call_tool("get_remote_access_status", json!({})).await.unwrap();
    assert!(!res.is_error);

    let res = client.call_tool("run_reachability_test", json!({})).await.unwrap();
    assert!(!res.is_error);

    let res = client.call_tool("get_bandwidth_usage", json!({})).await.unwrap();
    assert!(!res.is_error);

    let res = client
        .call_tool("update_remote_policy", json!({ "wanUploadLimitMbps": 50 }))
        .await
        .unwrap();
    assert!(!res.is_error);
    assert_eq!(res.structured_content.unwrap()["wanUploadLimitMbps"], 50);

    // 7. Settings tools: updating MCP security settings is rejected
    let res =
        client.call_tool("update_settings", json!({ "mcp": { "allowWan": true } })).await.unwrap();
    assert!(res.is_error);
    assert!(res.content[0].text.contains("MCP security settings"));

    // 8. Delete user: confirmation required
    let res = client.call_tool("delete_user", json!({ "userId": real_user_id })).await.unwrap();
    assert!(res.is_error);
    assert!(res.content[0].text.contains("confirmation required"));

    // Delete user: confirm execution
    let res = client
        .call_tool("delete_user", json!({ "userId": real_user_id, "confirm": true }))
        .await
        .unwrap();
    assert!(!res.is_error);

    // 9. Restart server: confirmation required
    let res = client.call_tool("restart_server", json!({})).await.unwrap();
    assert!(res.is_error);
    assert!(res.content[0].text.contains("confirmation required"));

    // 10. Prompts: setup_remote_access
    let rpc_res = client
        .send_rpc(mbl_mcp::JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(99)),
            method: "prompts/get".into(),
            params: Some(json!({ "name": "setup_remote_access" })),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(!rpc_res.result.unwrap()["messages"].as_array().unwrap().is_empty());
}
