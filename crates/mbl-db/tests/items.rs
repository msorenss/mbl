#![allow(clippy::unwrap_used)]

use camino::Utf8PathBuf;
use mbl_core::{Actor, ItemId, LibraryAccess, LibraryId, MediaKind, UserId};
use mbl_db::repo::items::{self, ItemFilter, ListQuery, SortField, SortOrder};
use mbl_db::{Db, DbOptions};

struct Fixture {
    db: Db,
    _dir: tempfile::TempDir,
    movies: LibraryId,
    shows: LibraryId,
    heat: ItemId,
    ep: Vec<ItemId>,
}

async fn exec(db: &Db, sql: &str) -> i64 {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(&db.write)
        .await
        .unwrap()
        .last_insert_rowid()
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("t.db")).unwrap();
    let db = Db::connect(&path, &DbOptions::default()).await.unwrap();
    exec(&db, "INSERT INTO users (id, username, display_name, password_hash, created_at) VALUES (1, 'u', 'u', 'x', 0)").await;
    let movies =
        exec(&db, "INSERT INTO libraries (name, kind, created_at) VALUES ('Movies', 'movie', 0)")
            .await;
    let shows =
        exec(&db, "INSERT INTO libraries (name, kind, created_at) VALUES ('Shows', 'show', 0)")
            .await;

    let mut ids = Vec::new();
    for (title, year, added, rating) in
        [("Heat", 1995, 30, 8.3), ("Alien", 1979, 10, 8.5), ("The Matrix", 1999, 20, 8.7)]
    {
        let sort = title.trim_start_matches("The ").to_lowercase();
        ids.push(exec(&db, &format!(
            "INSERT INTO media_items (library_id, kind, title, sort_title, year, rating, added_at, updated_at)
             VALUES ({movies}, 'movie', '{title}', '{sort}', {year}, {rating}, {added}, 0)"
        )).await);
    }
    let heat = ids[0];
    exec(&db, "INSERT INTO tags (id, kind, name) VALUES (1, 'genre', 'Crime')").await;
    exec(&db, &format!("INSERT INTO item_tags (item_id, tag_id) VALUES ({heat}, 1)")).await;
    exec(&db, "INSERT INTO people (id, name) VALUES (1, 'Al Pacino')").await;
    exec(&db, &format!("INSERT INTO item_people (item_id, person_id, role, character, ord) VALUES ({heat}, 1, 'actor', 'Vincent Hanna', 0)")).await;
    exec(&db, &format!("INSERT INTO artwork (item_id, kind, local_hash, blurhash, palette, selected) VALUES ({heat}, 'poster', 'abc', 'LKO2', '[\"#112233\"]', 1)")).await;
    let v = exec(&db, &format!("INSERT INTO media_versions (item_id, container, width, height, video_codec) VALUES ({heat}, 'mkv', 1920, 1080, 'h264')")).await;
    let p = exec(&db, &format!("INSERT INTO media_parts (version_id, path, size, mtime, probe) VALUES ({v}, '/m/heat.mkv', 100, 0, '{{}}')")).await;
    exec(&db, &format!("INSERT INTO streams (part_id, idx, kind, codec, language) VALUES ({p}, 0, 'video', 'h264', NULL), ({p}, 1, 'audio', 'aac', 'eng')")).await;

    let show = exec(&db, &format!("INSERT INTO media_items (library_id, kind, title, sort_title, added_at, updated_at) VALUES ({shows}, 'show', 'Severance', 'severance', 40, 0)")).await;
    let season = exec(&db, &format!("INSERT INTO media_items (library_id, kind, parent_id, title, sort_title, idx, added_at, updated_at) VALUES ({shows}, 'season', {show}, 'Season 1', 'season 1', 1, 40, 0)")).await;
    let mut ep = Vec::new();
    for n in 1..=3 {
        ep.push(ItemId(exec(&db, &format!("INSERT INTO media_items (library_id, kind, parent_id, title, sort_title, idx, parent_idx, added_at, updated_at) VALUES ({shows}, 'episode', {season}, 'Episode {n}', 'episode {n}', {n}, 1, 40, 0)")).await));
    }
    // Episode 1 watched, Heat half-way.
    exec(&db, &format!("INSERT INTO watch_state (user_id, item_id, view_count, last_viewed_at) VALUES (1, {}, 1, 100)", ep[0].0)).await;
    exec(&db, &format!("INSERT INTO watch_state (user_id, item_id, view_offset_ms, last_viewed_at) VALUES (1, {heat}, 60000, 200)")).await;

    Fixture {
        db,
        _dir: dir,
        movies: LibraryId(movies),
        shows: LibraryId(shows),
        heat: ItemId(heat),
        ep,
    }
}

fn user(access: LibraryAccess) -> ItemFilter {
    ItemFilter::for_user(&Actor {
        user_id: UserId(1),
        is_admin: false,
        library_access: access,
        is_managed: false,
        content_rating_ceiling: None,
    })
}

fn titles(items: &[items::ItemSummary]) -> Vec<&str> {
    items.iter().map(|i| i.title.as_str()).collect()
}

#[tokio::test]
async fn list_sorts_filters_and_paginates() {
    let f = fixture().await;
    let all = user(LibraryAccess::All);
    let q = ListQuery { limit: 2, ..Default::default() };
    let page = items::list(&f.db, &all, f.movies, &q).await.unwrap();
    assert_eq!(titles(&page.items), ["Alien", "Heat"]);
    let next = ListQuery { cursor: page.next_cursor.clone(), ..q };
    let page2 = items::list(&f.db, &all, f.movies, &next).await.unwrap();
    assert_eq!(titles(&page2.items), ["The Matrix"]);
    assert!(page2.next_cursor.is_none());

    let by_year = ListQuery {
        sort: SortField::Year,
        order: Some(SortOrder::Asc),
        limit: 10,
        ..Default::default()
    };
    assert_eq!(
        titles(&items::list(&f.db, &all, f.movies, &by_year).await.unwrap().items),
        ["Alien", "Heat", "The Matrix"]
    );

    let crime = ListQuery { genre: Some("crime".into()), limit: 10, ..Default::default() };
    let hits = items::list(&f.db, &all, f.movies, &crime).await.unwrap().items;
    assert_eq!(titles(&hits), ["Heat"]);
    assert_eq!(hits[0].poster_hash.as_deref(), Some("abc"));
    assert_eq!(hits[0].view_offset_ms, 60000);

    let unwatched = ListQuery {
        kind: Some(MediaKind::Episode),
        unwatched: true,
        limit: 10,
        ..Default::default()
    };
    assert_eq!(
        titles(&items::list(&f.db, &all, f.shows, &unwatched).await.unwrap().items),
        ["Episode 2", "Episode 3"]
    );
}

#[tokio::test]
async fn detail_includes_versions_people_artwork_and_watch_state() {
    let f = fixture().await;
    let d = items::detail(&f.db, &user(LibraryAccess::All), f.heat).await.unwrap().unwrap();
    assert_eq!(d.genres, ["Crime"]);
    assert_eq!(d.people[0].character.as_deref(), Some("Vincent Hanna"));
    assert_eq!(d.artwork[0].palette, ["#112233"]);
    assert_eq!(d.versions[0].parts[0].streams.len(), 2);
    assert_eq!(d.watch.view_offset_ms, 60000);
}

#[tokio::test]
async fn library_acl_hides_items_everywhere() {
    let f = fixture().await;
    let shows_only = user(LibraryAccess::Only(vec![f.shows]));
    assert!(items::detail(&f.db, &shows_only, f.heat).await.unwrap().is_none());
    assert!(items::search(&f.db, &shows_only, "heat", 10).await.unwrap().is_empty());
    assert!(items::continue_watching(&f.db, &shows_only, 20).await.unwrap().is_empty());
    assert!(
        items::list(&f.db, &shows_only, f.movies, &ListQuery { limit: 10, ..Default::default() })
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn search_and_hubs() {
    let f = fixture().await;
    let all = user(LibraryAccess::All);
    assert_eq!(titles(&items::search(&f.db, &all, "matr", 10).await.unwrap()), ["The Matrix"]);
    assert_eq!(items::search(&f.db, &all, "episode", 2).await.unwrap().len(), 2);
    assert_eq!(titles(&items::continue_watching(&f.db, &all, 20).await.unwrap()), ["Heat"]);
    let next = items::up_next(&f.db, &all, 20).await.unwrap();
    assert_eq!(next.iter().map(|i| i.id).collect::<Vec<_>>(), [f.ep[1]]);
    assert_eq!(
        titles(&items::recently_added(&f.db, &all, f.movies, 2).await.unwrap()),
        ["Heat", "The Matrix"]
    );
}
