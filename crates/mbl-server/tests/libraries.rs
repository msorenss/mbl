#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn admin_crud_lifecycle() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let movies = app.media_dir("Movies");
    let movies2 = app.media_dir("Movies2");

    let created = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(json!({ "name": " Movies ", "kind": "movie", "paths": [movies] })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["name"], "Movies");
    assert_eq!(created.body["kind"], "movie");
    let id = created.body["id"].as_i64().unwrap();
    let canonical = std::fs::canonicalize(&movies).unwrap();
    assert_eq!(created.body["paths"][0], canonical.to_str().unwrap());

    let updated = app
        .call(
            Method::PATCH,
            &format!("/api/v1/libraries/{id}"),
            Some(&admin),
            Some(json!({ "name": "Films", "paths": [movies, movies2] })),
        )
        .await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.body);
    assert_eq!(updated.body["name"], "Films");
    assert_eq!(updated.body["paths"].as_array().unwrap().len(), 2);

    let list = app
        .call(Method::GET, "/api/v1/libraries", Some(&admin), None)
        .await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);

    let del = app
        .call(
            Method::DELETE,
            &format!("/api/v1/libraries/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let gone = app
        .call(
            Method::GET,
            &format!("/api/v1/libraries/{id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn path_validation() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let root = app.media_dir("");
    let file = format!("{root}/file.mkv");
    std::fs::write(&file, "x").unwrap();

    for (paths, expected) in [
        (json!([]), StatusCode::UNPROCESSABLE_ENTITY),
        (json!(["relative/path"]), StatusCode::UNPROCESSABLE_ENTITY),
        (
            json!([format!("{root}/missing")]),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (json!([file]), StatusCode::UNPROCESSABLE_ENTITY),
    ] {
        let res = app
            .call(
                Method::POST,
                "/api/v1/libraries",
                Some(&admin),
                Some(json!({ "name": "X", "kind": "movie", "paths": paths })),
            )
            .await;
        assert_eq!(res.status, expected, "{paths} -> {:?}", res.body);
    }

    let bad_kind = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(json!({ "name": "X", "kind": "podcast", "paths": [root] })),
        )
        .await;
    assert!(bad_kind.status.is_client_error());
}

#[tokio::test]
async fn overlapping_roots_are_rejected() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let media = app.media_dir("");
    let movies = app.media_dir("Movies");
    let tv = app.media_dir("TV");

    let create = |name: &str, kind: &str, paths: serde_json::Value| json!({ "name": name, "kind": kind, "paths": paths });

    let first = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(create("Movies", "movie", json!([movies]))),
        )
        .await;
    assert_eq!(first.status, StatusCode::CREATED);

    let same = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(create("Dup", "movie", json!([movies]))),
        )
        .await;
    assert_eq!(same.status, StatusCode::CONFLICT);

    let parent = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(create("All", "movie", json!([media]))),
        )
        .await;
    assert_eq!(parent.status, StatusCode::CONFLICT);

    let nested_input = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(create(
                "Nested",
                "show",
                json!([tv, format!("{tv}/Season 1")]),
            )),
        )
        .await;
    assert!(nested_input.status.is_client_error());

    let tv_lib = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&admin),
            Some(create("TV", "show", json!([tv]))),
        )
        .await;
    assert_eq!(tv_lib.status, StatusCode::CREATED, "{:?}", tv_lib.body);
}

#[tokio::test]
async fn non_admins_can_read_but_not_write() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let user = app.create_user(&admin, "anna").await;
    let movies = app.media_dir("Movies");

    let forbidden = app
        .call(
            Method::POST,
            "/api/v1/libraries",
            Some(&user),
            Some(json!({ "name": "M", "kind": "movie", "paths": [movies] })),
        )
        .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    app.call(
        Method::POST,
        "/api/v1/libraries",
        Some(&admin),
        Some(json!({ "name": "M", "kind": "movie", "paths": [movies] })),
    )
    .await;
    let list = app
        .call(Method::GET, "/api/v1/libraries", Some(&user), None)
        .await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.body.as_array().unwrap().len(), 1);

    let browse = app
        .call(Method::GET, "/api/v1/system/browse", Some(&user), None)
        .await;
    assert_eq!(browse.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn folder_browser() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_dir("Movies");
    app.media_dir("TV");
    let media = app.dir.path().join("media");

    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/system/browse?path={}", media.to_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let names: Vec<_> = res.body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].clone())
        .collect();
    assert_eq!(names, [json!("Movies"), json!("TV")]);

    let roots = app
        .call(Method::GET, "/api/v1/system/browse", Some(&admin), None)
        .await;
    assert!(roots.body["path"].is_null());
    assert!(!roots.body["entries"].as_array().unwrap().is_empty());
}
