//! Planned movie nights: invited people are members and notified, voting closes at the planned
//! time and the most liked title wins.
#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, seed_playback_fixtures};
use mbl_core::events::Event;
use serde_json::json;

#[tokio::test]
async fn a_planned_movie_night_closes_at_its_time() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = ?")
        .bind(seeded.movie_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let up: i64 = sqlx::query_scalar(
        "INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at) \
         VALUES (?, 'movie', 'Up', 'up', 100, 100) RETURNING id",
    )
    .bind(library)
    .fetch_one(&db.write)
    .await
    .unwrap();
    let anna = app.create_user(&admin, "anna").await;
    let bo = app.create_user(&admin, "bo").await;
    let ids = app.call(Method::GET, "/api/v1/users", Some(&admin), None).await;
    let id_of = |name: &str| {
        ids.body.as_array().unwrap().iter().find(|u| u["username"] == name).unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    let (anna_id, bo_id) = (id_of("anna"), id_of("bo"));
    let filter = json!({ "libraryId": library }).to_string();
    let soon = mbl_core::time::now_ms() + 3_600_000;

    // A time in the past, too far ahead, or an unknown user is refused.
    for body in [
        json!({ "filterConfig": filter, "startsAt": 1000 }),
        json!({ "filterConfig": filter, "startsAt": soon + 20 * 86_400_000 }),
        json!({ "filterConfig": filter, "startsAt": soon, "inviteUserIds": [9999] }),
    ] {
        let res =
            app.call(Method::POST, "/api/v1/matchmaker/sessions", Some(&admin), Some(body)).await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    }

    let mut events = services.events.subscribe();
    let res = app
        .call(
            Method::POST,
            "/api/v1/matchmaker/sessions",
            Some(&admin),
            Some(json!({ "filterConfig": filter, "startsAt": soon, "inviteUserIds": [anna_id, bo_id, anna_id] })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{}", res.body);
    assert_eq!(res.body["startsAt"], soon);
    assert_eq!(res.body["memberCount"], 3);
    let session = res.body["id"].as_str().unwrap().to_owned();
    match events.recv().await.unwrap() {
        Event::MovieNightScheduled { session_id, starts_at, user_ids, .. } => {
            assert_eq!(session_id, session);
            assert_eq!(starts_at, soon);
            assert_eq!(user_ids.len(), 3);
        }
        other => panic!("unexpected event {other:?}"),
    }

    // Invited people are members without joining. Nobody agrees on everything.
    let vote = |token: String, item: i64| {
        let app = &app;
        let path = format!("/api/v1/matchmaker/sessions/{session}/vote");
        async move {
            app.call(
                Method::POST,
                &path,
                Some(&token),
                Some(json!({ "userName": "", "itemId": item, "vote": 1 })),
            )
            .await
        }
    };
    assert_eq!(vote(admin.clone(), seeded.movie_id).await.body["matched"], false);
    assert_eq!(vote(anna.clone(), up).await.body["matched"], false);
    assert_eq!(vote(bo.clone(), up).await.body["matched"], false);

    // The time comes: the most liked title wins.
    sqlx::query("UPDATE matchmaker_sessions SET starts_at = ? WHERE id = ?")
        .bind(mbl_core::time::now_ms() - 1000)
        .bind(&session)
        .execute(&db.write)
        .await
        .unwrap();
    assert_eq!(services.matchmaker.decide_due().await.unwrap(), 1);
    let res = app
        .call(Method::GET, &format!("/api/v1/matchmaker/sessions/{session}"), Some(&bo), None)
        .await;
    assert_eq!(res.body["status"], "matched");
    assert_eq!(res.body["matchedItemId"], up);
    match events.recv().await.unwrap() {
        Event::MovieNightDecided { item_id, title, user_ids, .. } => {
            assert_eq!(item_id.map(|i| i.0), Some(up));
            assert_eq!(title.as_deref(), Some("Up"));
            assert_eq!(user_ids.len(), 3);
        }
        other => panic!("unexpected event {other:?}"),
    }

    // A planned night nobody liked anything in closes without a pick; joining and voting stop.
    let res = app
        .call(
            Method::POST,
            "/api/v1/matchmaker/sessions",
            Some(&admin),
            Some(json!({ "filterConfig": filter, "startsAt": soon })),
        )
        .await;
    let empty = res.body["id"].as_str().unwrap().to_owned();
    sqlx::query("UPDATE matchmaker_sessions SET starts_at = ? WHERE id = ?")
        .bind(mbl_core::time::now_ms() - 1000)
        .bind(&empty)
        .execute(&db.write)
        .await
        .unwrap();
    let res = app
        .call(Method::POST, &format!("/api/v1/matchmaker/sessions/{empty}/join"), Some(&anna), None)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    // Reading decides it even before the background check runs.
    let res = app
        .call(Method::GET, &format!("/api/v1/matchmaker/sessions/{empty}"), Some(&admin), None)
        .await;
    assert_eq!(res.body["status"], "closed");
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/matchmaker/sessions/{empty}/vote"),
            Some(&admin),
            Some(json!({ "userName": "", "itemId": up, "vote": 1 })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);
}
