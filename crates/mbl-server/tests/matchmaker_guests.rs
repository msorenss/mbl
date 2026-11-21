//! Movie night guests: people without an account join through the host's link, vote with a token
//! and see only what the host may see.
#![allow(clippy::unwrap_used)]

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{Res, TestApp, seed_playback_fixtures};
use serde_json::{Value, json};

async fn as_guest(
    app: &TestApp,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> Res {
    let req = Request::builder().method(method).uri(uri).header("x-mbl-guest", token);
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    app.send(req).await
}

#[tokio::test]
async fn guests_join_with_a_link_and_vote_with_their_token() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = ?")
        .bind(seeded.movie_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    // Heat is rated R; the host may only see up to PG-13, so guests get Up but not Heat.
    sqlx::query("UPDATE media_items SET content_rating = 'R' WHERE id = ?")
        .bind(seeded.movie_id)
        .execute(&db.write)
        .await
        .unwrap();
    let up: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, content_rating, added_at, updated_at) \
         VALUES (?, 'movie', 'Up', 'up', 'PG', 100, 100) RETURNING id",
    )
    .bind(library)
    .fetch_one(&db.write)
    .await
    .unwrap();
    let res = app
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({
                "username": "host", "password": "Password123!",
                "contentRatingCeiling": "PG-13", "libraryIds": [library]
            })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    let host = app.login("host", "Password123!").await.body["token"].as_str().unwrap().to_owned();
    let member = app.create_user(&admin, "member").await;

    let res = app
        .call(
            Method::POST,
            "/api/v1/matchmaker/sessions",
            Some(&host),
            Some(json!({ "filterConfig": json!({ "libraryId": library }).to_string() })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    let session = res.body["id"].as_str().unwrap().to_owned();
    let guests = format!("/api/v1/matchmaker/sessions/{session}/guests");

    // Only the host (or an admin) invites guests; a member who joined may not.
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/matchmaker/sessions/{session}/join"),
            Some(&member),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert_eq!(
        app.call(Method::POST, &guests, Some(&member), None).await.status,
        StatusCode::FORBIDDEN
    );
    // An unknown code finds nothing.
    let res = app
        .call(
            Method::POST,
            "/api/v1/matchmaker/guest/join/nope",
            None,
            Some(json!({ "name": "A" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    let res = app.call(Method::POST, &guests, Some(&host), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let code = res.body["code"].as_str().unwrap().to_owned();
    assert_eq!(app.call(Method::POST, &guests, Some(&host), None).await.body["code"], code);

    let join = format!("/api/v1/matchmaker/guest/join/{code}");
    let res = app.call(Method::POST, &join, None, Some(json!({ "name": "   " }))).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let res = app.call(Method::POST, &join, None, Some(json!({ "name": "  Anna   B " }))).await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    assert_eq!(res.body["name"], "Anna B");
    let token = res.body["token"].as_str().unwrap().to_owned();

    // The guest sees what the host may see.
    let res = as_guest(&app, Method::GET, "/api/v1/matchmaker/guest", "wrong", None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    let res = as_guest(&app, Method::GET, "/api/v1/matchmaker/guest", &token, None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let titles: Vec<&str> = res.body["titles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Up"), "{titles:?}");
    assert!(!titles.contains(&"Heat"), "an R-rated title for a PG-13 host: {titles:?}");
    assert_eq!(res.body["memberCount"], 3);
    assert_eq!(res.body["seed"], session);
    let res = as_guest(
        &app,
        Method::POST,
        "/api/v1/matchmaker/guest/vote",
        &token,
        Some(json!({ "itemId": seeded.movie_id, "vote": 1 })),
    )
    .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND, "a title the host may not see");

    // Everyone (host, member and guest) likes Up: a match.
    for voter in [&host, &member] {
        let res = app
            .call(
                Method::POST,
                &format!("/api/v1/matchmaker/sessions/{session}/vote"),
                Some(voter),
                Some(json!({ "userName": "", "itemId": up, "vote": 1 })),
            )
            .await;
        assert_eq!(res.body["matched"], false, "{}", res.body);
    }
    let vote = json!({ "itemId": up, "vote": 1 });
    let res =
        as_guest(&app, Method::POST, "/api/v1/matchmaker/guest/vote", &token, Some(vote)).await;
    assert_eq!(res.body, json!({ "matched": true, "matchedItemId": up }));
    let res = as_guest(&app, Method::GET, "/api/v1/matchmaker/guest", &token, None).await;
    assert_eq!(res.body["matched"]["title"], "Up");

    // Posters need a valid token.
    let res =
        app.call(Method::GET, "/api/v1/matchmaker/guest/images/1?token=wrong", None, None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);

    // Turning guests off removes them.
    assert_eq!(
        app.call(Method::DELETE, &guests, Some(&host), None).await.status,
        StatusCode::NO_CONTENT
    );
    let res = as_guest(&app, Method::GET, "/api/v1/matchmaker/guest", &token, None).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    let res = app.call(Method::POST, &join, None, Some(json!({ "name": "Late" }))).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}
