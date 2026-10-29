#![allow(clippy::unwrap_used)]

use camino::Utf8PathBuf;
use mbl_db::repo::settings::SettingsRepo;
use mbl_db::{Db, DbOptions};

fn temp_db_path(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().join("mbl.db")).unwrap()
}

async fn table_exists(db: &Db, name: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = ?)")
        .bind(name)
        .fetch_one(&db.read)
        .await
        .unwrap()
}

#[tokio::test]
async fn creates_database_and_applies_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let path = temp_db_path(&dir);
    let db = Db::connect(&path, &DbOptions::default()).await.unwrap();

    assert!(path.exists());
    for table in ["users", "media_items", "media_parts", "jobs", "settings", "items_fts"] {
        assert!(table_exists(&db, table).await, "missing table {table}");
    }
    assert!(!dir.path().join("backups").exists(), "fresh DB must not be backed up");
}

#[tokio::test]
async fn pragmas_are_applied() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::connect(&temp_db_path(&dir), &DbOptions { cache_kib: 4096, read_connections: 2 })
        .await
        .unwrap();

    let mode: String =
        sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&db.write).await.unwrap();
    assert_eq!(mode, "wal");
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&db.read).await.unwrap();
    assert_eq!(fk, 1);
    let cache: i64 = sqlx::query_scalar("PRAGMA cache_size").fetch_one(&db.read).await.unwrap();
    assert_eq!(cache, -4096);
    let busy: i64 = sqlx::query_scalar("PRAGMA busy_timeout").fetch_one(&db.write).await.unwrap();
    assert_eq!(busy, 5000);
}

#[tokio::test]
async fn read_pool_is_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::connect(&temp_db_path(&dir), &DbOptions::default()).await.unwrap();
    let res =
        sqlx::query("INSERT INTO settings (key, value) VALUES ('a', 'b')").execute(&db.read).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn settings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::connect(&temp_db_path(&dir), &DbOptions::default()).await.unwrap();
    let settings = SettingsRepo::new(db);

    assert_eq!(settings.get("theme").await.unwrap(), None);
    settings.set("theme", "dark").await.unwrap();
    settings.set("theme", "oled").await.unwrap();
    assert_eq!(settings.get("theme").await.unwrap().as_deref(), Some("oled"));
    assert!(settings.delete("theme").await.unwrap());
    assert!(!settings.delete("theme").await.unwrap());
}

#[tokio::test]
async fn concurrent_reads_and_writes() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::connect(&temp_db_path(&dir), &DbOptions::default()).await.unwrap();
    let settings = SettingsRepo::new(db);

    let mut tasks = Vec::new();
    for w in 0..4 {
        let s = settings.clone();
        tasks.push(tokio::spawn(async move {
            for i in 0..50 {
                s.set(&format!("w{w}-{i}"), "v").await.unwrap();
            }
        }));
    }
    for _ in 0..4 {
        let s = settings.clone();
        tasks.push(tokio::spawn(async move {
            for i in 0..50 {
                let _ = s.get(&format!("w0-{i}")).await.unwrap();
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    assert_eq!(settings.get("w3-49").await.unwrap().as_deref(), Some("v"));
}

#[tokio::test]
async fn existing_unmigrated_db_is_backed_up_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = temp_db_path(&dir);

    // Simulate an older database that predates the pending migrations.
    let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(&path).create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
    sqlx::query("CREATE TABLE legacy (id INTEGER)").execute(&pool).await.unwrap();
    pool.close().await;

    let db = Db::connect(&path, &DbOptions::default()).await.unwrap();
    db.close().await;
    let backups = || std::fs::read_dir(dir.path().join("backups")).unwrap().count();
    assert_eq!(backups(), 1);

    let db = Db::connect(&path, &DbOptions::default()).await.unwrap();
    db.close().await;
    assert_eq!(backups(), 1, "no backup when nothing is pending");
}

#[tokio::test]
async fn invitation_redemption_is_atomic_and_failed_creation_does_not_consume_it() {
    use mbl_db::repo::{
        m6,
        users::{NewUser, UsersRepo},
    };
    let dir = tempfile::tempdir().unwrap();
    let db = Db::connect(&temp_db_path(&dir), &DbOptions::default()).await.unwrap();
    let now = mbl_core::time::now_ms();
    let users = UsersRepo::new(db.clone());
    let admin = users
        .insert(
            &NewUser {
                username: "admin",
                display_name: "Admin",
                password_hash: "test",
                is_admin: true,
            },
            now,
        )
        .await
        .unwrap();
    let library: i64 = sqlx::query_scalar(
        "INSERT INTO libraries (name, kind, created_at) VALUES ('Movies', 'movie', ?) RETURNING id",
    )
    .bind(now)
    .fetch_one(&db.write)
    .await
    .unwrap();
    m6::create_invite(
        &db,
        "single-use",
        admin,
        &[mbl_core::LibraryId(library)],
        Some("PG"),
        now + 60_000,
    )
    .await
    .unwrap();
    // A username collision rolls back the transaction, leaving the link usable.
    let duplicate = NewUser {
        username: "admin",
        display_name: "Duplicate",
        password_hash: "test",
        is_admin: false,
    };
    assert!(m6::register_with_invite(&db, "single-use", &duplicate, now).await.is_err());
    assert!(m6::get_invite_by_code(&db, "single-use").await.unwrap().unwrap().used_at.is_none());
    let alice = NewUser {
        username: "alice",
        display_name: "Alice",
        password_hash: "test",
        is_admin: false,
    };
    let bob =
        NewUser { username: "bob", display_name: "Bob", password_hash: "test", is_admin: false };
    let (a, b) = tokio::join!(
        m6::register_with_invite(&db, "single-use", &alice, now),
        m6::register_with_invite(&db, "single-use", &bob, now)
    );
    assert_ne!(a.is_ok(), b.is_ok(), "redemptions: {a:?}, {b:?}");
    let winner = a.or(b).unwrap();
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db.read).await.unwrap();
    assert_eq!(total, 2);
    let security = m6::get_user_security_info(&db, winner).await.unwrap().unwrap();
    assert_eq!(security.content_rating_ceiling.as_deref(), Some("PG"));
    assert_eq!(security.library_ids, vec![mbl_core::LibraryId(library)]);
    assert_eq!(
        m6::get_invite_by_code(&db, "single-use").await.unwrap().unwrap().used_by,
        Some(winner)
    );
}
