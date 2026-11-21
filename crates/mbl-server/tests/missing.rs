//! Missing episodes and collection movies compared with a fake TMDB.
#![allow(clippy::unwrap_used)]

mod common;

use axum::Router;
use axum::extract::Path;
use axum::http::{Method, StatusCode};
use axum::routing::get;
use common::{TestApp, seed_playback_fixtures};
use serde_json::json;

async fn fake_tmdb() -> String {
    let router = Router::new()
        .route(
            "/tv/77",
            get(|| async {
                axum::Json(json!({
                    "seasons": [
                        { "season_number": 0, "episode_count": 3, "name": "Specials" },
                        { "season_number": 1, "episode_count": 4, "name": "Season 1" },
                        { "season_number": 2, "episode_count": 6, "name": "Season 2" },
                        { "season_number": 3, "episode_count": 6, "name": "Season 3" }
                    ],
                    "last_episode_to_air": { "season_number": 2, "episode_number": 1 }
                }))
            }),
        )
        .route(
            "/tv/77/season/{n}",
            get(|Path(n): Path<u32>| async move {
                let episodes: Vec<_> = (1..=6)
                    .map(|e| json!({ "episode_number": e, "name": format!("S{n} E{e}"), "air_date": "2020-01-01" }))
                    .collect();
                axum::Json(json!({ "episodes": episodes }))
            }),
        )
        .route(
            "/movie/949",
            get(|| async {
                axum::Json(json!({ "id": 949, "belongs_to_collection": { "id": 10, "name": "Heat Collection" } }))
            }),
        )
        .route(
            "/collection/10",
            get(|| async {
                axum::Json(json!({ "parts": [
                    { "id": 949, "title": "Heat", "release_date": "1995-12-15" },
                    { "id": 950, "title": "Heat 2", "release_date": "2020-01-01" },
                    { "id": 951, "title": "Heat 3", "release_date": "2099-01-01" }
                ]}))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn missing_episodes_and_collection_movies() {
    let (app, db, _) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let seeded = seed_playback_fixtures(&app, &db, &admin).await;
    let path = format!("/api/v1/items/{}/missing", seeded.show_id);

    sqlx::query("UPDATE media_items SET tmdb_id = 77 WHERE id = ?")
        .bind(seeded.show_id)
        .execute(&db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE media_items SET tmdb_id = 949 WHERE id = ?")
        .bind(seeded.movie_id)
        .execute(&db.write)
        .await
        .unwrap();

    // Without a TMDB key there is nothing to compare with.
    let res = app.call(Method::GET, &path, Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);

    let base = fake_tmdb().await;
    for (key, value) in [("tmdb_api_key", "key"), ("tmdb_base_url", base.as_str())] {
        mbl_db::repo::settings::SettingsRepo::new(db.clone()).set(key, value).await.unwrap();
    }
    // The library has S1E1–E2. Missing: S1E3–E4 and S2E1 (aired so far); not specials or S3.
    let res = app.call(Method::GET, &path, Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["missingCount"], 3, "{}", res.body);
    let seasons = res.body["seasons"].as_array().unwrap();
    assert_eq!(seasons.len(), 2);
    assert_eq!(seasons[0]["season"], 1);
    assert_eq!(seasons[0]["whole"], false);
    assert_eq!(
        seasons[0]["episodes"][0],
        json!({ "episode": 3, "title": "S1 E3", "airDate": "2020-01-01" })
    );
    assert_eq!(seasons[1]["season"], 2);
    assert_eq!(seasons[1]["whole"], true);

    // A movie is not a show.
    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/items/{}/missing", seeded.movie_id),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    // The movie library lacks Heat 2 (released) but not Heat 3 (not yet released).
    let library: i64 = sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = ?")
        .bind(seeded.movie_id)
        .fetch_one(&db.read)
        .await
        .unwrap();
    let res = app
        .call(Method::GET, &format!("/api/v1/libraries/{library}/missing"), Some(&admin), None)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(
        res.body["movies"],
        json!([{ "tmdbId": 950, "title": "Heat 2", "year": 2020, "collection": "Heat Collection", "poster": null }])
    );
    assert_eq!(res.body["checked"], 1);
}
