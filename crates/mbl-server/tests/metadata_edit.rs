//! Manual metadata editing: fields, locks, re-match and artwork choice.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use serde_json::json;

#[tokio::test]
async fn admin_edits_locks_matches_and_chooses_artwork() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let path = format!("/api/v1/items/{}/metadata", seeded.movie_id);

    let res = app.call(Method::GET, &path, Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["title"], "Heat");
    assert_eq!(res.body["canMatch"], true);
    assert_eq!(res.body["lockedFields"], json!([]));

    // Only administrators edit metadata.
    let viewer = app.create_user(&admin, "viewer").await;
    assert_eq!(
        app.call(Method::GET, &path, Some(&viewer), None).await.status,
        StatusCode::FORBIDDEN
    );

    // Changed fields are stored and locked; the sort title follows a new title.
    let res = app
        .call(
            Method::PATCH,
            &path,
            Some(&admin),
            Some(json!({
                "title": "The Heat", "summary": "  New summary ", "year": 1996,
                "genres": ["Crime", "crime", " Thriller "], "tagline": ""
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["title"], "The Heat");
    assert_eq!(res.body["sortTitle"], "heat");
    assert_eq!(res.body["summary"], "New summary");
    assert_eq!(res.body["year"], 1996);
    assert!(res.body["tagline"].is_null());
    assert_eq!(res.body["genres"], json!(["Crime", "Thriller"]));
    let locked = res.body["lockedFields"].as_array().unwrap().clone();
    for f in ["title", "summary", "year", "genres", "tagline"] {
        assert!(locked.contains(&json!(f)), "{f} in {locked:?}");
    }
    let detail = app
        .call(Method::GET, &format!("/api/v1/items/{}", seeded.movie_id), Some(&admin), None)
        .await;
    assert_eq!(detail.body["title"], "The Heat");
    assert_eq!(detail.body["summary"], "New summary");

    // The lock list can be replaced, which unlocks fields.
    let res = app
        .call(Method::PATCH, &path, Some(&admin), Some(json!({ "lockedFields": ["title"] })))
        .await;
    assert_eq!(res.body["lockedFields"], json!(["title"]));

    // Invalid input is refused without changes.
    for body in
        [json!({ "title": "  " }), json!({ "year": 3000 }), json!({ "lockedFields": ["rating"] })]
    {
        let res = app.call(Method::PATCH, &path, Some(&admin), Some(body.clone())).await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{body} {}", res.body);
    }

    // Episodes cannot be re-matched; movies are pointed at the chosen TMDB entry.
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/match", seeded.ep1_id),
            Some(&admin),
            Some(json!({ "tmdbId": 5 })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/items/{}/match", seeded.movie_id),
            Some(&admin),
            Some(json!({ "tmdbId": 949 })),
        )
        .await;
    assert_eq!(res.status, StatusCode::ACCEPTED, "{}", res.body);
    let res = app.call(Method::GET, &path, Some(&admin), None).await;
    assert_eq!(res.body["tmdbId"], 949);

    // Without a TMDB key there are no candidates; only TMDB image URLs can be chosen.
    let art = format!("/api/v1/items/{}/artwork", seeded.movie_id);
    let res = app.call(Method::GET, &format!("{art}?kind=poster"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["available"], false);
    for url in ["http://127.0.0.1/secret.jpg", "https://image.tmdb.org/t/p/original/../x.jpg"] {
        let res = app
            .call(Method::PUT, &art, Some(&admin), Some(json!({ "kind": "poster", "url": url })))
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{url}");
    }
    let chosen = "https://image.tmdb.org/t/p/original/abc123.jpg";
    let res = app
        .call(Method::PUT, &art, Some(&admin), Some(json!({ "kind": "poster", "url": chosen })))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
    let res = app.call(Method::GET, &format!("{art}?kind=poster"), Some(&admin), None).await;
    assert_eq!(res.body["current"], chosen);
    let res = app.call(Method::GET, &path, Some(&admin), None).await;
    assert!(res.body["lockedFields"].as_array().unwrap().contains(&json!("poster")));
}
