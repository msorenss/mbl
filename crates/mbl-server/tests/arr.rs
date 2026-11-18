//! Radarr/Sonarr: settings, connection test, adding on approval and auto-available.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::Query;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::get;
use common::TestApp;
use serde_json::{Value, json};
use std::collections::HashMap;

type Posted = Arc<Mutex<Vec<(String, Value)>>>;

/// A fake Radarr (`app` = "Radarr") or Sonarr that checks the API key and records added titles.
async fn fake_arr(app: &'static str) -> (String, Posted) {
    let posted: Posted = Arc::default();
    let key_ok = |h: &HeaderMap| h.get("x-api-key").and_then(|v| v.to_str().ok()) == Some("secret");
    let store = posted.clone();
    let add = move |path: &'static str| {
        let store = store.clone();
        move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| {
            let store = store.clone();
            async move {
                if !key_ok(&headers) {
                    return (StatusCode::UNAUTHORIZED, axum::Json(Value::Null));
                }
                if body["qualityProfileId"].is_null() || body["rootFolderPath"].is_null() {
                    return (
                        StatusCode::BAD_REQUEST,
                        axum::Json(json!([{ "errorMessage": "Missing profile or folder" }])),
                    );
                }
                store.lock().unwrap().push((path.to_owned(), body.clone()));
                let mut created = body;
                created["id"] = json!(77);
                (StatusCode::CREATED, axum::Json(created))
            }
        }
    };
    let router = Router::new()
        .route(
            "/api/v3/system/status",
            get(move |h: HeaderMap| async move {
                if key_ok(&h) {
                    (StatusCode::OK, axum::Json(json!({ "appName": app, "version": "5.1.0" })))
                } else {
                    (StatusCode::UNAUTHORIZED, axum::Json(Value::Null))
                }
            }),
        )
        .route(
            "/api/v3/rootfolder",
            get(|| async { axum::Json(json!([{ "path": "/media/movies" }])) }),
        )
        .route(
            "/api/v3/qualityprofile",
            get(|| async { axum::Json(json!([{ "id": 4, "name": "HD-1080p" }])) }),
        )
        .route(
            "/api/v3/movie/lookup/tmdb",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                // TMDB 1 is already in Radarr (has an id).
                let id = if q["tmdbId"] == "1" { 5 } else { 0 };
                axum::Json(json!({ "title": "Interstellar", "tmdbId": q["tmdbId"], "id": id }))
            }),
        )
        .route(
            "/api/v3/series/lookup",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                assert!(q["term"].starts_with("tmdb:"));
                axum::Json(json!([{ "title": "Dark Matter", "tvdbId": 424_242 }]))
            }),
        )
        .route("/api/v3/movie", axum::routing::post(add("movie")))
        .route("/api/v3/series", axum::routing::post(add("series")));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (format!("http://{addr}"), posted)
}

async fn request(app: &TestApp, token: &str, kind: &str, tmdb: i64, title: &str) -> i64 {
    let res = app
        .call(
            Method::POST,
            "/api/v1/requests",
            Some(token),
            Some(json!({ "kind": kind, "tmdbId": tmdb, "title": title })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    res.body["id"].as_i64().unwrap()
}

async fn approve(app: &TestApp, token: &str, id: i64) -> common::Res {
    app.call(
        Method::PATCH,
        &format!("/api/v1/requests/{id}/status"),
        Some(token),
        Some(json!({ "status": "approved" })),
    )
    .await
}

async fn find(app: &TestApp, token: &str, id: i64) -> Value {
    let res = app.call(Method::GET, "/api/v1/requests", Some(token), None).await;
    res.body.as_array().unwrap().iter().find(|r| r["id"] == id).unwrap().clone()
}

#[tokio::test]
async fn radarr_settings_test_and_add_on_approval() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let (url, posted) = fake_arr("Radarr").await;

    // Turning it on without folder/profile is refused.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/requests/arr/radarr",
            Some(&admin),
            Some(json!({ "enabled": true, "url": url, "apiKey": "secret", "rootFolder": "" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);

    // Wrong key, then a Sonarr address, are reported; the right one lists folders and profiles.
    let test = |key: &str, path: &str| {
        let body = json!({ "url": url, "apiKey": key });
        let path = path.to_owned();
        let app = &app;
        let admin = admin.clone();
        async move { app.call(Method::POST, &path, Some(&admin), Some(body)).await }
    };
    let res = test("wrong", "/api/v1/requests/arr/radarr/test").await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(res.body.to_string().contains("rejected"), "{}", res.body);
    let res = test("secret", "/api/v1/requests/arr/sonarr/test").await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let res = test("secret", "/api/v1/requests/arr/radarr/test").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["rootFolders"], json!(["/media/movies"]));
    assert_eq!(res.body["qualityProfiles"][0]["id"], 4);

    let res = app
        .call(
            Method::PUT,
            "/api/v1/requests/arr/radarr",
            Some(&admin),
            Some(json!({
                "enabled": true, "url": format!("{url}/"), "apiKey": "secret",
                "rootFolder": "/media/movies", "qualityProfileId": 4
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["hasApiKey"], true);
    assert!(res.body.get("apiKey").is_none(), "the key must not be returned");
    assert_eq!(res.body["url"], url);

    // A non-admin cannot read the settings.
    let user = app.create_user(&admin, "viewer").await;
    let res = app.call(Method::GET, "/api/v1/requests/arr/radarr", Some(&user), None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);

    // Approving adds the movie with search on and stores Radarr's ID.
    let id = request(&app, &user, "movie", 157_336, "Interstellar").await;
    assert_eq!(approve(&app, &admin, id).await.status, StatusCode::NO_CONTENT);
    let row = find(&app, &admin, id).await;
    assert_eq!(row["status"], "approved");
    assert_eq!(row["arrId"], 77);
    let sent = posted.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1["qualityProfileId"], 4);
    assert_eq!(sent[0].1["rootFolderPath"], "/media/movies");
    assert_eq!(sent[0].1["addOptions"]["searchForMovie"], true);

    // Already in Radarr: approved with the existing ID, nothing posted.
    let id = request(&app, &user, "movie", 1, "Already there").await;
    assert_eq!(approve(&app, &admin, id).await.status, StatusCode::NO_CONTENT);
    assert_eq!(find(&app, &admin, id).await["arrId"], 5);
    assert_eq!(posted.lock().unwrap().len(), 1);

    // Sonarr is off: a show request is only marked approved.
    let id = request(&app, &user, "show", 2, "Some Show").await;
    assert_eq!(approve(&app, &admin, id).await.status, StatusCode::NO_CONTENT);
    assert!(find(&app, &admin, id).await["arrId"].is_null());

    // Sending it again while Sonarr is off says so; once Sonarr is on, the resend adds it.
    let res = approve(&app, &admin, id).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    assert!(res.body.to_string().contains("Sonarr is turned off"), "{}", res.body);
    let res = app
        .call(
            Method::PUT,
            "/api/v1/requests/arr/sonarr",
            Some(&admin),
            Some(json!({
                "enabled": true, "url": url, "apiKey": "secret",
                "rootFolder": "/media/movies", "qualityProfileId": 4
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(approve(&app, &admin, id).await.status, StatusCode::NO_CONTENT);
    assert_eq!(find(&app, &admin, id).await["arrId"], 77);
    assert_eq!(posted.lock().unwrap().len(), 2);
    // A sent request is not sent twice.
    assert_eq!(approve(&app, &admin, id).await.status, StatusCode::NO_CONTENT);
    assert_eq!(posted.lock().unwrap().len(), 2);

    // When the movie shows up in a library the request becomes available.
    sqlx::query("INSERT INTO libraries (id, name, kind, created_at) VALUES (90, 'L', 'movie', 0)")
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_items (library_id, kind, title, sort_title, tmdb_id, added_at, updated_at)
         VALUES (90, 'movie', 'Interstellar', 'interstellar', 157336, 0, 0)",
    )
    .execute(&db.write)
    .await
    .unwrap();
    let interstellar = find(&app, &admin, 1).await;
    assert_eq!(interstellar["status"], "available");

    // Unknown status is rejected.
    let res = app
        .call(
            Method::PATCH,
            &format!("/api/v1/requests/{id}/status"),
            Some(&admin),
            Some(json!({ "status": "downloaded" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn sonarr_looks_up_by_tmdb_and_failure_keeps_status() {
    let (app, _db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let (url, posted) = fake_arr("Sonarr").await;
    let res = app
        .call(
            Method::PUT,
            "/api/v1/requests/arr/sonarr",
            Some(&admin),
            Some(json!({
                "enabled": true, "url": url, "apiKey": "secret",
                "rootFolder": "/media/tv", "qualityProfileId": 4
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);

    let id = request(&app, &admin, "show", 196_322, "Dark Matter").await;
    assert_eq!(approve(&app, &admin, id).await.status, StatusCode::NO_CONTENT);
    let sent = posted.lock().unwrap().clone();
    assert_eq!(sent[0].0, "series");
    assert_eq!(sent[0].1["tvdbId"], 424_242);
    assert_eq!(sent[0].1["addOptions"]["searchForMissingEpisodes"], true);
    assert_eq!(find(&app, &admin, id).await["arrId"], 77);

    // Key changed in Sonarr: approval fails with the reason and the request stays pending.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/requests/arr/sonarr",
            Some(&admin),
            Some(json!({
                "enabled": true, "url": url, "apiKey": "stale",
                "rootFolder": "/media/tv", "qualityProfileId": 4
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let id = request(&app, &admin, "show", 3, "Other Show").await;
    let res = approve(&app, &admin, id).await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(res.body.to_string().contains("rejected"), "{}", res.body);
    assert_eq!(find(&app, &admin, id).await["status"], "pending");
}

/// A fake Lidarr (API v1) that records added artists and albums.
async fn fake_lidarr() -> (String, Posted) {
    let posted: Posted = Arc::default();
    let store = posted.clone();
    let add = move |path: &'static str| {
        let store = store.clone();
        move |axum::Json(body): axum::Json<Value>| {
            let store = store.clone();
            async move {
                store.lock().unwrap().push((path.to_owned(), body.clone()));
                let mut created = body;
                created["id"] = json!(31);
                (StatusCode::CREATED, axum::Json(created))
            }
        }
    };
    const ABBA: &str = "d87e52c5-bb8d-4da8-b941-9f4928627dc8";
    const ARRIVAL: &str = "a3f1f4c5-6d0b-3c5e-8d2a-0123456789ab";
    let router = Router::new()
        .route(
            "/api/v1/system/status",
            get(|| async { axum::Json(json!({ "appName": "Lidarr", "version": "2.8.0" })) }),
        )
        .route("/api/v1/rootfolder", get(|| async { axum::Json(json!([{ "path": "/music" }])) }))
        .route(
            "/api/v1/qualityprofile",
            get(|| async { axum::Json(json!([{ "id": 1, "name": "Lossless" }])) }),
        )
        .route(
            "/api/v1/metadataprofile",
            get(|| async { axum::Json(json!([{ "id": 2, "name": "Standard" }])) }),
        )
        .route(
            "/api/v1/artist/lookup",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                let found = json!({ "artistName": "ABBA", "foreignArtistId": ABBA,
                    "images": [{ "coverType": "poster", "remoteUrl": "http://img/abba.jpg" }] });
                // Searching by name and by `lidarr:<mbid>` both find the artist.
                assert!(q["term"] == "abba" || q["term"] == format!("lidarr:{ABBA}"), "{q:?}");
                axum::Json(json!([found]))
            }),
        )
        .route(
            "/api/v1/album/lookup",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                assert!(q["term"] == "arrival" || q["term"] == format!("lidarr:{ARRIVAL}"));
                axum::Json(json!([{ "title": "Arrival", "foreignAlbumId": ARRIVAL,
                    "releaseDate": "1976-10-11T00:00:00Z",
                    "artist": { "artistName": "ABBA", "foreignArtistId": ABBA } }]))
            }),
        )
        .route("/api/v1/artist", axum::routing::post(add("artist")))
        .route("/api/v1/album", axum::routing::post(add("album")));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (format!("http://{addr}"), posted)
}

#[tokio::test]
async fn lidarr_music_search_and_requests() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let user = app.create_user(&admin, "listener").await;

    // Without Lidarr, music search explains what is missing.
    let res = app
        .call(
            Method::GET,
            "/api/v1/requests/music/search?query=abba&kind=artist",
            Some(&user),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(res.body.to_string().contains("Lidarr"), "{}", res.body);

    let (url, posted) = fake_lidarr().await;
    let res = app
        .call(
            Method::POST,
            "/api/v1/requests/arr/lidarr/test",
            Some(&admin),
            Some(json!({ "url": url, "apiKey": "k" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["metadataProfiles"][0]["id"], 2);

    // Lidarr needs a metadata profile before it can be turned on.
    let settings = |metadata: Value| {
        json!({ "enabled": true, "url": url, "apiKey": "k", "rootFolder": "/music",
                "qualityProfileId": 1, "metadataProfileId": metadata })
    };
    let res = app
        .call(Method::PUT, "/api/v1/requests/arr/lidarr", Some(&admin), Some(settings(Value::Null)))
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let res = app
        .call(Method::PUT, "/api/v1/requests/arr/lidarr", Some(&admin), Some(settings(json!(2))))
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["metadataProfileId"], 2);

    // Any signed-in user can search; hits carry the MusicBrainz ID.
    let res = app
        .call(
            Method::GET,
            "/api/v1/requests/music/search?query=abba&kind=artist",
            Some(&user),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let abba = &res.body[0];
    assert_eq!(abba["title"], "ABBA");
    assert_eq!(abba["imageUrl"], "http://img/abba.jpg");
    let res = app
        .call(
            Method::GET,
            "/api/v1/requests/music/search?query=arrival&kind=album",
            Some(&user),
            None,
        )
        .await;
    let album = &res.body[0];
    assert_eq!((album["artist"].as_str(), album["year"].as_i64()), (Some("ABBA"), Some(1976)));

    // A music request needs a MusicBrainz ID.
    let res = app
        .call(
            Method::POST,
            "/api/v1/requests",
            Some(&user),
            Some(json!({ "kind": "artist", "title": "ABBA" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    let create = |body: Value| {
        let app = &app;
        let user = user.clone();
        async move {
            let res = app.call(Method::POST, "/api/v1/requests", Some(&user), Some(body)).await;
            assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
            res.body["id"].as_i64().unwrap()
        }
    };
    let artist = create(
        json!({ "kind": "artist", "title": "ABBA", "musicbrainzId": abba["musicbrainzId"] }),
    )
    .await;
    let album_id = create(json!({ "kind": "album", "title": "Arrival", "artist": "ABBA",
        "musicbrainzId": album["musicbrainzId"], "year": 1976 }))
    .await;

    assert_eq!(approve(&app, &admin, artist).await.status, StatusCode::NO_CONTENT);
    assert_eq!(approve(&app, &admin, album_id).await.status, StatusCode::NO_CONTENT);
    let sent = posted.lock().unwrap().clone();
    assert_eq!(sent[0].0, "artist");
    assert_eq!(sent[0].1["metadataProfileId"], 2);
    assert_eq!(sent[0].1["rootFolderPath"], "/music");
    assert_eq!(sent[0].1["addOptions"]["searchForMissingAlbums"], true);
    assert_eq!(sent[1].0, "album");
    assert_eq!(sent[1].1["artist"]["qualityProfileId"], 1);
    assert_eq!(sent[1].1["artist"]["addOptions"]["monitor"], "none");
    assert_eq!(sent[1].1["addOptions"]["searchForNewAlbum"], true);
    assert_eq!(find(&app, &admin, album_id).await["arrId"], 31);

    // The album turns available when the scanner has it under the right artist.
    sqlx::query("INSERT INTO libraries (id, name, kind, created_at) VALUES (91, 'M', 'music', 0)")
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_items (id, library_id, kind, title, sort_title, added_at, updated_at)
         VALUES (500, 91, 'artist', 'ABBA', 'abba', 0, 0)",
    )
    .execute(&db.write)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media_items (library_id, kind, parent_id, title, sort_title, added_at, updated_at)
         VALUES (91, 'album', 500, 'arrival', 'arrival', 0, 0)",
    )
    .execute(&db.write)
    .await
    .unwrap();
    assert_eq!(find(&app, &admin, album_id).await["status"], "available");
    assert_eq!(find(&app, &admin, artist).await["status"], "available");
}
