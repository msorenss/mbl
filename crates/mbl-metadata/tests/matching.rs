#![allow(clippy::unwrap_used)]

use std::path::Path;

use camino::Utf8PathBuf;
use mbl_core::events::EventBus;
use mbl_core::{ItemId, LibraryId, LibraryKind, MediaKind, time::now_ms};
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::media::{self, NewItem, NewPart, VersionSummary};
use mbl_db::{Db, DbOptions};
use mbl_metadata::{Matcher, TmdbClient, TmdbConfig};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Env {
    db: Db,
    dir: tempfile::TempDir,
    lib: LibraryId,
}

async fn env(kind: LibraryKind) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let db_path = Utf8PathBuf::from_path_buf(dir.path().join("mbl.db")).unwrap();
    let db = Db::connect(&db_path, &DbOptions::default()).await.unwrap();
    let root = dir.path().join("media");
    std::fs::create_dir_all(&root).unwrap();
    let lib = LibrariesRepo::new(db.clone())
        .insert("Lib", kind, &[root.to_str().unwrap().to_owned()], now_ms())
        .await
        .unwrap();
    Env { db, dir, lib }
}

fn item<'a>(
    lib: LibraryId,
    kind: MediaKind,
    parent: Option<ItemId>,
    title: &'a str,
) -> NewItem<'a> {
    NewItem {
        library_id: lib,
        kind: Some(kind),
        parent_id: parent,
        title,
        sort_title: title,
        year: None,
        idx: None,
        parent_idx: None,
        originally_available_at: None,
        tmdb_id: None,
        tvdb_id: None,
        imdb_id: None,
        duration_ms: None,
    }
}

/// Create an item with one file at `rel` (relative to the media root).
async fn with_file(e: &Env, new: NewItem<'_>, rel: &str) -> ItemId {
    let file = e.dir.path().join("media").join(rel);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"video").unwrap();
    let mut tx = e.db.write.begin().await.unwrap();
    let id = media::insert_item(&mut tx, &new, now_ms()).await.unwrap();
    let v = media::insert_version(&mut tx, id, &VersionSummary::default()).await.unwrap();
    media::insert_part(
        &mut tx,
        &NewPart {
            version_id: v,
            path: file.to_str().unwrap(),
            size: 5,
            mtime: 0,
            quick_hash: None,
            duration_ms: None,
            probe: "{}",
            chapters: "[]",
            part_index: 0,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn client(server: &MockServer) -> TmdbClient {
    let mut config = TmdbConfig::new("test-key", "en-US");
    config.base_url = server.uri();
    TmdbClient::new(config).unwrap()
}

fn heat_details() -> serde_json::Value {
    json!({
        "id": 949, "title": "Heat", "original_title": "Heat",
        "overview": "Obsessive master thief Neil McCauley...", "tagline": "A Los Angeles Crime Saga",
        "release_date": "1995-12-15", "vote_average": 7.9,
        "genres": [{ "id": 80, "name": "Crime" }, { "id": 18, "name": "Drama" }],
        "production_companies": [{ "name": "Regency Enterprises" }],
        "poster_path": "/heat.jpg", "backdrop_path": "/heat-bg.jpg", "imdb_id": "tt0113277",
        "credits": {
            "cast": [
                { "id": 1158, "name": "Al Pacino", "character": "Vincent Hanna", "profile_path": "/al.jpg" },
                { "id": 380, "name": "Robert De Niro", "character": "Neil McCauley" }
            ],
            "crew": [
                { "id": 638, "name": "Michael Mann", "job": "Director" },
                { "id": 9, "name": "Someone", "job": "Gaffer" }
            ]
        }
    })
}

async fn row(db: &Db, id: ItemId) -> (String, Option<String>, Option<i64>, Option<f64>, i64) {
    sqlx::query_as("SELECT title, summary, tmdb_id, match_confidence, needs_review FROM media_items WHERE id = ?")
        .bind(id)
        .fetch_one(&db.read)
        .await
        .unwrap()
}

#[tokio::test]
async fn movie_is_matched_via_search() {
    let e = env(LibraryKind::Movie).await;
    let mut new = item(e.lib, MediaKind::Movie, None, "Heat");
    new.year = Some(1995);
    let id = with_file(&e, new, "Heat (1995)/Heat (1995).mkv").await;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .and(query_param("query", "Heat"))
        .and(query_param("year", "1995"))
        .and(query_param("api_key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "results": [
            { "id": 1, "title": "Heat Wave", "release_date": "2022-01-01" },
            { "id": 949, "title": "Heat", "original_title": "Heat", "release_date": "1995-12-15" }
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/movie/949"))
        .respond_with(ResponseTemplate::new(200).set_body_json(heat_details()))
        .expect(1)
        .mount(&server)
        .await;

    let matcher = Matcher::new(e.db.clone(), Some(client(&server).await), EventBus::default());
    matcher.match_item(id).await.unwrap();

    let (title, summary, tmdb, confidence, review) = row(&e.db, id).await;
    assert_eq!(title, "Heat");
    assert!(summary.unwrap().starts_with("Obsessive"));
    assert_eq!(tmdb, Some(949));
    assert!(confidence.unwrap() > 0.95);
    assert_eq!(review, 0);

    let people: Vec<(String, String)> = sqlx::query_as(
        "SELECT p.name, ip.role FROM item_people ip JOIN people p ON p.id = ip.person_id WHERE ip.item_id = ? ORDER BY ip.ord",
    )
    .bind(id)
    .fetch_all(&e.db.read)
    .await
    .unwrap();
    assert_eq!(people.len(), 3, "two actors + director, gaffer ignored: {people:?}");
    let genres: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM item_tags it JOIN tags t ON t.id = it.tag_id WHERE it.item_id = ? AND t.kind = 'genre' ORDER BY t.name",
    )
    .bind(id)
    .fetch_all(&e.db.read)
    .await
    .unwrap();
    assert_eq!(genres, ["Crime", "Drama"]);
    let poster: String = sqlx::query_scalar(
        "SELECT source_url FROM artwork WHERE item_id = ? AND kind = 'poster' AND selected = 1",
    )
    .bind(id)
    .fetch_one(&e.db.read)
    .await
    .unwrap();
    assert_eq!(poster, "https://image.tmdb.org/t/p/original/heat.jpg");
}

#[tokio::test]
async fn low_confidence_needs_review_and_locked_fields_survive() {
    let e = env(LibraryKind::Movie).await;
    let mut new = item(e.lib, MediaKind::Movie, None, "Big Test");
    new.year = Some(2020);
    let id = with_file(&e, new, "Big Test (2020)/Big Test (2020).mkv").await;
    sqlx::query("UPDATE media_items SET locked_fields = '[\"title\"]' WHERE id = ?")
        .bind(id)
        .execute(&e.db.write)
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search/movie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "results": [
            { "id": 5, "title": "Totally Unrelated", "release_date": "1980-01-01" }
        ]})))
        .mount(&server)
        .await;

    let matcher = Matcher::new(e.db.clone(), Some(client(&server).await), EventBus::default());
    matcher.match_item(id).await.unwrap();
    let (title, _, tmdb, confidence, review) = row(&e.db, id).await;
    assert_eq!(title, "Big Test");
    assert_eq!(tmdb, None);
    assert!(confidence.unwrap() < 0.85);
    assert_eq!(review, 1);
}

#[tokio::test]
async fn nfo_id_skips_search_and_nfo_values_win() {
    let e = env(LibraryKind::Movie).await;
    let id =
        with_file(&e, item(e.lib, MediaKind::Movie, None, "heat"), "Heat (1995)/Heat (1995).mkv")
            .await;
    let dir = e.dir.path().join("media/Heat (1995)");
    std::fs::write(
        dir.join("Heat (1995).nfo"),
        "<movie><title>Heat (Director's Cut)</title><uniqueid type=\"tmdb\">949</uniqueid></movie>",
    )
    .unwrap();
    std::fs::write(dir.join("poster.jpg"), b"jpg").unwrap();

    let server = MockServer::start().await;
    Mock::given(path("/search/movie"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(path("/movie/949"))
        .respond_with(ResponseTemplate::new(200).set_body_json(heat_details()))
        .expect(1)
        .mount(&server)
        .await;

    let matcher = Matcher::new(e.db.clone(), Some(client(&server).await), EventBus::default());
    matcher.match_item(id).await.unwrap();
    let (title, summary, tmdb, confidence, _) = row(&e.db, id).await;
    assert_eq!(title, "Heat (Director's Cut)", "NFO title beats TMDB");
    assert!(summary.is_some(), "TMDB fills fields the NFO lacks");
    assert_eq!((tmdb, confidence), (Some(949), Some(1.0)));
    let posters: Vec<String> = sqlx::query_scalar(
        "SELECT source_url FROM artwork WHERE item_id = ? AND kind = 'poster' ORDER BY id",
    )
    .bind(id)
    .fetch_all(&e.db.read)
    .await
    .unwrap();
    assert!(posters[0].starts_with("file:") && posters[0].ends_with("poster.jpg"), "{posters:?}");
    assert_eq!(posters.len(), 2);
}

#[tokio::test]
async fn without_api_key_only_local_metadata_is_used() {
    let e = env(LibraryKind::Movie).await;
    let id =
        with_file(&e, item(e.lib, MediaKind::Movie, None, "Heat"), "Heat (1995)/movie.mkv").await;
    std::fs::write(
        e.dir.path().join("media/Heat (1995)/movie.nfo"),
        "<movie><title>Heat</title><plot>Local plot</plot><genre>Crime</genre></movie>",
    )
    .unwrap();
    Matcher::new(e.db.clone(), None, EventBus::default()).match_item(id).await.unwrap();
    let (_, summary, tmdb, confidence, review) = row(&e.db, id).await;
    assert_eq!(summary.as_deref(), Some("Local plot"));
    assert_eq!((tmdb, confidence, review), (None, None, 0));
}

#[tokio::test]
async fn show_and_episodes_are_matched() {
    let e = env(LibraryKind::Show).await;
    let show =
        with_file(&e, item(e.lib, MediaKind::Show, None, "Test Show"), "Test Show/show.keep").await;
    let mut tx = e.db.write.begin().await.unwrap();
    let mut season = item(e.lib, MediaKind::Season, Some(show), "Season 1");
    season.idx = Some(1);
    let season_id = media::insert_item(&mut tx, &season, now_ms()).await.unwrap();
    let mut ep = item(e.lib, MediaKind::Episode, Some(season_id), "Episode 1");
    ep.idx = Some(1);
    let ep_id = media::insert_item(&mut tx, &ep, now_ms()).await.unwrap();
    tx.commit().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(path("/search/tv"))
        .and(query_param("query", "Test Show"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "results": [
            { "id": 77, "name": "Test Show", "first_air_date": "2019-02-01" }
        ]})))
        .mount(&server)
        .await;
    Mock::given(path("/tv/77"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 77, "name": "Test Show", "overview": "A show.", "first_air_date": "2019-02-01",
            "genres": [{ "name": "Comedy" }], "networks": [{ "name": "MBL TV" }],
            "external_ids": { "imdb_id": "tt7777", "tvdb_id": 7777 }
        })))
        .mount(&server)
        .await;
    Mock::given(path("/tv/77/season/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "overview": "First season.", "poster_path": "/s1.jpg",
            "episodes": [{ "episode_number": 1, "name": "Pilot", "overview": "It begins.", "air_date": "2019-02-01", "still_path": "/e1.jpg" }]
        })))
        .mount(&server)
        .await;

    let matcher = Matcher::new(e.db.clone(), Some(client(&server).await), EventBus::default());
    matcher.match_item(show).await.unwrap();

    let (_, summary, tmdb, _, _) = row(&e.db, show).await;
    assert_eq!((summary.as_deref(), tmdb), (Some("A show."), Some(77)));
    let tvdb: Option<i64> = sqlx::query_scalar("SELECT tvdb_id FROM media_items WHERE id = ?")
        .bind(show)
        .fetch_one(&e.db.read)
        .await
        .unwrap();
    assert_eq!(tvdb, Some(7777));
    let (title, summary, _, _, _) = row(&e.db, ep_id).await;
    assert_eq!((title.as_str(), summary.as_deref()), ("Pilot", Some("It begins.")));
    let still: String =
        sqlx::query_scalar("SELECT source_url FROM artwork WHERE item_id = ? AND kind = 'thumb'")
            .bind(ep_id)
            .fetch_one(&e.db.read)
            .await
            .unwrap();
    assert!(still.ends_with("/e1.jpg"));
}

#[tokio::test]
async fn responses_are_cached_on_disk() {
    let server = MockServer::start().await;
    Mock::given(path("/search/movie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "results": [] })))
        .expect(1)
        .mount(&server)
        .await;
    let cache = tempfile::tempdir().unwrap();
    let mut config = TmdbConfig::new("k", "en-US");
    config.base_url = server.uri();
    config.cache_dir = Some(cache.path().to_path_buf());
    let c = TmdbClient::new(config).unwrap();
    assert!(c.search_movie("Nothing", None).await.unwrap().is_empty());
    assert!(c.search_movie("Nothing", None).await.unwrap().is_empty());
    assert!(std::fs::read_dir(cache.path()).unwrap().count() == 1);
    assert!(!Path::new(cache.path()).join("missing").exists());
}

#[tokio::test]
async fn bad_api_key_is_an_external_error() {
    let server = MockServer::start().await;
    Mock::given(path("/search/movie"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = client(&server).await.search_movie("x", None).await.unwrap_err();
    assert!(matches!(err, mbl_core::Error::External(_)), "{err:?}");
}
