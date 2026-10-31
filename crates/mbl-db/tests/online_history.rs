#![allow(clippy::unwrap_used)]
use mbl_db::{
    Db, DbOptions,
    repo::{
        online::OnlineRepo,
        users::{NewUser, UsersRepo},
    },
};
#[tokio::test]
async fn online_history_is_private_bounded_and_independent_of_discovery_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("test.db")).unwrap();
    let db = Db::connect(&path, &DbOptions::default()).await.unwrap();
    let users = UsersRepo::new(db.clone());
    let owner = users
        .insert(
            &NewUser {
                username: "owner",
                display_name: "Owner",
                password_hash: "unused",
                is_admin: false,
            },
            1,
        )
        .await
        .unwrap();
    let other = users
        .insert(
            &NewUser {
                username: "other",
                display_name: "Other",
                password_hash: "unused",
                is_admin: false,
            },
            1,
        )
        .await
        .unwrap();
    let repo = OnlineRepo::new(db.clone());
    let source:mbl_core::online::OnlineSource=serde_json::from_value(serde_json::json!({"id":"source","url":"https://example.org/episode","title":"Episode","kind":"episode","episodes":[],"availability":"unchecked","detail":"Fixture"})).unwrap();
    repo.save(owner, &source).await.unwrap();
    repo.progress(owner, &source, 42.0, 120.0).await.unwrap();
    sqlx::query("DELETE FROM online_sources").execute(&db.write).await.unwrap();
    assert_eq!(repo.history(owner).await.unwrap()[0].position, 42.0);
    assert!(repo.history(other).await.unwrap().is_empty());
    for i in 0..205 {
        let mut next = source.clone();
        next.id = format!("source-{i}");
        repo.progress(owner, &next, 1.0, 120.0).await.unwrap();
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM online_watch WHERE owner_id=?")
        .bind(owner)
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(count, 200);
    users.delete(owner).await.unwrap();
    assert!(repo.history(owner).await.unwrap().is_empty());
}
