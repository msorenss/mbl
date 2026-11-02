#![allow(clippy::unwrap_used)]
use std::collections::HashMap;
use std::sync::{
    Mutex as StdMutex, OnceLock,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use mbl_core::online::OnlineAvailability;
use serde_json::json;

use super::http::{OnlineHttp, public_ip};
use super::*;

#[derive(Debug, Clone)]
struct Reply {
    status: u16,
    kind: String,
    body: Vec<u8>,
}
#[derive(Debug, Default)]
struct FakeHttp {
    replies: StdMutex<HashMap<String, Reply>>,
    calls: AtomicUsize,
}
impl FakeHttp {
    fn set(&self, url: &str, status: u16, kind: &str, body: Vec<u8>) {
        self.replies.lock().unwrap().insert(url.into(), Reply { status, kind: kind.into(), body });
    }
    fn search(&self, hits: serde_json::Value) {
        self.set(
            SEARCH_URL,
            200,
            "application/json",
            json!({"results":hits}).to_string().into_bytes(),
        );
    }
}
#[async_trait]
impl OnlineHttp for FakeHttp {
    async fn fetch(&self, url: Url, limit: usize, _prefix: bool) -> Result<Fetched> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let key = if url.host_str() == Some("api.mwmbl.org") { SEARCH_URL } else { url.as_str() };
        let reply = self
            .replies
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| Error::Unavailable("Missing fixture".into()))?;
        Ok(Fetched {
            url,
            status: reply.status,
            content_type: reply.kind,
            body: reply.body.into_iter().take(limit).collect(),
        })
    }
}
async fn fixture() -> (tempfile::TempDir, OnlineService, Arc<FakeHttp>, Actor) {
    let dir = tempfile::tempdir().unwrap();
    let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("online.db")).unwrap();
    let db = Db::connect(&path, &mbl_db::DbOptions::default()).await.unwrap();
    let user_id = mbl_db::repo::users::UsersRepo::new(db.clone())
        .insert(
            &mbl_db::repo::users::NewUser {
                username: "viewer",
                display_name: "Viewer",
                password_hash: "unused",
                is_admin: false,
            },
            mbl_core::time::now_ms(),
        )
        .await
        .unwrap();
    let mut service =
        OnlineService::new(db).with_tools(Some(mbl_probe::FfTools::locate("", "").await.unwrap()));
    service.plugins.update_config("webdrama", true, json!({})).await.unwrap();
    let http = Arc::new(FakeHttp::default());
    service.http = http.clone();
    (dir, service, http, Actor::user(user_id))
}
fn clip() -> Vec<u8> {
    static VIDEO: OnceLock<Vec<u8>> = OnceLock::new();
    VIDEO
        .get_or_init(|| {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("clip.mp4");
            let status = std::process::Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-y",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=64x64:r=2",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=24000",
                    "-t",
                    "1",
                    "-c:v",
                    "libx264",
                    "-c:a",
                    "aac",
                    "-threads",
                    "1",
                    "-pix_fmt",
                    "yuv420p",
                    "-movflags",
                    "+faststart",
                ])
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
            std::fs::read(path).unwrap()
        })
        .clone()
}

#[test]
fn blocks_private_reserved_credential_and_non_http_source_urls() {
    for url in [
        "file:///etc/passwd",
        "ftp://example.org/a",
        "http://localhost/a",
        "http://localhost./a",
        "http://127.1/a",
        "http://2130706433/a",
        "http://169.254.169.254/a",
        "http://100.64.0.1/a",
        "http://192.168.1.1/a",
        "http://[::1]/",
        "http://[::ffff:127.0.0.1]/",
        "http://[2002:7f00:1::]/",
        "https://user:password@example.org/",
        "http://example.org:8000/a",
    ] {
        assert!(public_url(url).is_err(), "{url}");
    }
    for ip in [
        "0.0.0.0",
        "10.0.0.1",
        "172.16.0.1",
        "192.0.0.8",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.2",
        "203.0.113.1",
        "224.0.0.1",
        "240.0.0.1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
        "2001:0:1::1",
        "64:ff9b::a00:1",
    ] {
        assert!(!public_ip(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["8.8.8.8", "1.1.1.1", "2001:4860:4860::8888"] {
        assert!(public_ip(ip.parse().unwrap()));
    }
    assert_eq!(
        public_url("https://example.org/show#episode").unwrap().as_str(),
        "https://example.org/show"
    );
}

#[tokio::test]
async fn public_fetch_rejects_private_literal_before_networking() {
    for url in ["http://127.0.0.1/", "http://[::1]/"] {
        assert!(matches!(
            PublicHttp.fetch(Url::parse(url).unwrap(), 1024, false).await,
            Err(Error::Validation(_))
        ));
    }
}

#[tokio::test]
async fn search_deduplicates_unsafe_urls_caches_and_limits_external_requests() {
    let (_dir, service, http, actor) = fixture().await;
    http.search(json!([
        {"url":"https://example.org/show#one","title":"Show\u{202e}","content":"Stream page"},
        {"url":"https://example.org/show#two","title":"Duplicate"},
        {"url":"http://127.0.0.1/private","title":"Private"},
        {"url":"javascript:alert(1)","title":"Script"}
    ]));
    let first = service.search(&actor, "Some show").await.unwrap();
    assert_eq!(first.results.len(), 1);
    assert_eq!(first.results[0].title, "Show");
    assert_eq!(first.results[0].availability, Availability::Unchecked);
    assert_eq!(first.results[0].media_url, None);
    service.search(&actor, "Some show").await.unwrap();
    assert_eq!(http.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(service.search(&actor, "Another show").await, Err(Error::RateLimited { .. })));
    assert!(matches!(service.search(&actor, " ").await, Err(Error::Validation(_))));
    assert!(matches!(service.search(&actor, &"x".repeat(161)).await, Err(Error::Validation(_))));
}

#[tokio::test]
async fn restrictions_and_disabled_plugin_apply_before_network_or_cache() {
    let (_dir, service, http, actor) = fixture().await;
    http.search(json!([]));
    service.search(&actor, "A show").await.unwrap();
    let mut restricted = actor.clone();
    restricted.is_managed = true;
    assert!(matches!(service.search(&restricted, "A show").await, Err(Error::Forbidden)));
    assert!(matches!(
        service.inspect(&restricted, "https://example.org/show").await,
        Err(Error::Forbidden)
    ));
    restricted.is_managed = false;
    restricted.library_access = LibraryAccess::Only(vec![]);
    assert!(matches!(service.search(&restricted, "A show").await, Err(Error::Forbidden)));
    restricted.library_access = LibraryAccess::All;
    restricted.content_rating_ceiling = Some("PG".into());
    assert!(matches!(service.search(&restricted, "A show").await, Err(Error::Forbidden)));
    service.plugins.update_config("webdrama", false, json!({})).await.unwrap();
    assert!(matches!(service.search(&actor, "A show").await, Err(Error::Unavailable(_))));
    assert!(matches!(
        service.inspect(&actor, "https://example.org/show").await,
        Err(Error::Unavailable(_))
    ));
    assert_eq!(http.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn structured_series_metadata_preserves_seasons_orders_episodes_and_ignores_private_links() {
    let mut result = source("https://example.org/show");
    let html = r#"<title>Fallback</title><script type='application/ld+json'>{"@type":"TVSeries","name":"Fixture series","containsSeason":[{"@type":"TVSeason","seasonNumber":2,"episode":[{"@type":"TVEpisode","name":"Second","episodeNumber":2,"url":"/s2e2"},{"@type":"TVEpisode","name":"First","episodeNumber":1,"url":"/s2e1"},{"@type":"TVEpisode","name":"Bad","url":"http://127.0.0.1/private"}]}]}</script>"#;
    let base = Url::parse(&result.url).unwrap();
    assert!(metadata::extract(&mut result, &base, html).is_none());
    assert_eq!(result.kind, "series");
    assert_eq!(result.series_title.as_deref(), Some("Fixture series"));
    assert_eq!(result.episodes.len(), 2);
    assert_eq!(result.episodes[0].episode_number, Some(1));
    assert_eq!(result.episodes[0].season_number, Some(2));
    assert_eq!(result.episodes[0].url, "https://example.org/s2e1");
}

#[tokio::test]
async fn inspect_indexes_real_episode_metadata_and_decodes_a_media_sample() {
    let (_dir, service, http, actor) = fixture().await;
    http.set("https://example.org/episode",200,"text/html",br#"<script type="application/ld+json">{"@type":"TVEpisode","name":"Fixture episode","episodeNumber":3,"partOfSeason":{"seasonNumber":2},"partOfSeries":{"name":"Fixture show"},"associatedMedia":{"@type":"VideoObject","contentUrl":"/clip.mp4"}}</script>"#.to_vec());
    http.set("https://example.org/clip.mp4", 206, "video/mp4", clip());
    let result = service.inspect(&actor, "https://example.org/episode").await.unwrap();
    assert_eq!(result.availability, OnlineAvailability::MediaDetected);
    assert_eq!(result.has_video, Some(true));
    assert_eq!(result.series_title.as_deref(), Some("Fixture show"));
    assert_eq!(result.season_number, Some(2));
    assert_eq!(result.episode_number, Some(3));
    assert_eq!(result.media_url.as_deref(), Some("https://example.org/clip.mp4"));
    assert_eq!(
        service.repo.get(actor.user_id, &result.id).await.unwrap().unwrap().title,
        "Fixture episode"
    );
    http.search(json!([]));
    let search = service.search(&actor, "Fixture show").await.unwrap();
    assert_eq!(search.indexed.len(), 1);
    http.set("https://example.org/episode", 403, "text/html", b"Sign in".to_vec());
    let failed = service.inspect(&actor, "https://example.org/episode").await.unwrap();
    assert_eq!(failed.availability, Availability::Restricted);
    assert!(failed.media_url.is_none());
    assert!(
        service.repo.get(actor.user_id, &result.id).await.unwrap().unwrap().media_url.is_none()
    );
}

#[tokio::test]
async fn pages_hls_restricted_and_invalid_media_never_claim_verified_playback() {
    let (_dir, service, http, actor) = fixture().await;
    for (path, code, kind, body, status) in [
        ("/article", 200, "text/html", b"<title>News</title>".to_vec(), Availability::ExternalPage),
        (
            "/stream",
            200,
            "application/vnd.apple.mpegurl",
            b"#EXTM3U\nhttp://127.0.0.1/private\n".to_vec(),
            Availability::ExternalPlayer,
        ),
        ("/denied", 403, "text/html", b"Blocked".to_vec(), Availability::Restricted),
        ("/missing", 404, "text/html", b"Missing".to_vec(), Availability::Unavailable),
        ("/broken", 200, "video/mp4", b"Not media".to_vec(), Availability::Unknown),
    ] {
        let url = format!("https://example.org{path}");
        http.set(&url, code, kind, body);
        let result = service.inspect(&actor, &url).await.unwrap();
        assert_eq!(result.availability, status);
        assert!(result.media_url.is_none());
    }
    assert_eq!(http.calls.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn decoder_cannot_follow_manifest_file_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private.ts");
    std::fs::write(&path, clip()).unwrap();
    let playlist = format!(
        "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\n{}\n#EXT-X-ENDLIST\n",
        path.display()
    );
    let tools = mbl_probe::FfTools::locate("", "").await.unwrap();
    assert!(probe::media_sample(playlist.as_bytes(), Some(&tools)).await.is_err());
    assert!(probe::media_sample(b"not a video", Some(&tools)).await.is_err());
}

#[tokio::test]
async fn stale_verification_and_busy_checks_do_not_offer_old_playback() {
    let (_dir, service, http, actor) = fixture().await;
    let mut old = source("https://example.org/old");
    old.availability = Availability::MediaDetected;
    old.media_url = Some("https://example.org/old.mp4".into());
    old.checked_at = Some(mbl_core::time::now_ms() - 600_000);
    let stale = fresh(old);
    assert_eq!(stale.availability, Availability::Unchecked);
    assert!(stale.media_url.is_none());
    let _busy = service.inspections.acquire_many(2).await.unwrap();
    assert!(matches!(
        service.inspect(&actor, "https://example.org/new").await,
        Err(Error::RateLimited { .. })
    ));
    assert_eq!(http.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn inspected_metadata_and_signed_urls_are_private_to_the_inspecting_user() {
    let (dir, service, http, actor) = fixture().await;
    let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("online.db")).unwrap();
    let db = Db::connect(&path, &mbl_db::DbOptions::default()).await.unwrap();
    let other_id = mbl_db::repo::users::UsersRepo::new(db.clone())
        .insert(
            &mbl_db::repo::users::NewUser {
                username: "other",
                display_name: "Other",
                password_hash: "unused",
                is_admin: false,
            },
            mbl_core::time::now_ms(),
        )
        .await
        .unwrap();
    let other = Actor::user(other_id);
    let signed = "https://example.org/episode?token=private-access";
    http.set(signed, 200, "text/html", b"<title>Private episode</title>".to_vec());
    let checked = service.inspect(&actor, signed).await.unwrap();
    assert!(service.repo.get(other.user_id, &checked.id).await.unwrap().is_none());
    http.search(json!([]));
    assert_eq!(service.search(&actor, "Private episode").await.unwrap().indexed.len(), 1);
    assert!(service.search(&other, "Private episode").await.unwrap().indexed.is_empty());
    http.set(signed, 200, "text/html", b"<title>Second viewer metadata</title>".to_vec());
    service.inspect(&other, signed).await.unwrap();
    assert_eq!(
        service.repo.get(actor.user_id, &checked.id).await.unwrap().unwrap().title,
        "Private episode"
    );
    assert_eq!(
        service.repo.get(other.user_id, &checked.id).await.unwrap().unwrap().title,
        "Second viewer metadata"
    );
    mbl_db::repo::users::UsersRepo::new(db).delete(actor.user_id).await.unwrap();
    assert!(service.repo.get(actor.user_id, &checked.id).await.unwrap().is_none());
    assert!(service.repo.get(other.user_id, &checked.id).await.unwrap().is_some());
}

#[tokio::test]
async fn source_checks_honor_configured_tools_and_never_fallback_to_path() {
    let (dir, mut service, http, actor) = fixture().await;
    let url = "https://example.org/direct.mp4";
    http.set(url, 200, "video/mp4", clip());
    service = service.with_tools(None);
    assert_eq!(service.inspect(&actor, url).await.unwrap().availability, Availability::Unknown);
    service = service.with_tools(Some(mbl_probe::FfTools {
        ffprobe: dir.path().join("missing-probe"),
        ffmpeg: dir.path().join("missing-decoder"),
    }));
    let result = service.inspect(&actor, url).await.unwrap();
    assert_eq!(result.availability, Availability::Unknown);
    assert!(result.media_url.is_none());
}

#[tokio::test]
async fn searxng_uses_json_streaming_query_and_configuration_scopes_cache() {
    let (_dir, service, http, actor) = fixture().await;
    http.search(json!([{"url":"https://example.org/old","title":"Old"}]));
    service.search(&actor, "Some show").await.unwrap();
    service
        .plugins
        .update_config(
            "webdrama",
            true,
            json!({"searchProvider":"searxng","searxngUrl":"https://search.example.org/base"}),
        )
        .await
        .unwrap();
    *service.last_search.lock().await = None;
    let mut endpoint = Url::parse("https://search.example.org/base/search").unwrap();
    endpoint
        .query_pairs_mut()
        .append_pair("q", "Some show watch online")
        .append_pair("format", "json")
        .append_pair("categories", "general,videos");
    http.set(endpoint.as_str(),200,"application/json",json!({"results":[{"url":"https://example.org/watch","title":"Some show watch episodes","content":"Streaming"}]}).to_string().into_bytes());
    let result = service.search(&actor, "Some show").await.unwrap();
    assert_eq!(result.provider, "SearXNG");
    assert_eq!(result.results[0].url, "https://example.org/watch");
    assert_eq!(result.results[0].availability, Availability::Unchecked);
    assert_eq!(http.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn own_index_survives_provider_failure_and_normalizes_title_punctuation() {
    let (_dir, service, http, actor) = fixture().await;
    http.set(
        "https://example.org/show",
        200,
        "text/html",
        b"<title>Single's Inferno</title>".to_vec(),
    );
    service.inspect(&actor, "https://example.org/show").await.unwrap();
    http.set(SEARCH_URL, 503, "text/html", vec![]);
    let result = service.search(&actor, "Singles Inferno").await.unwrap();
    assert!(result.warning.is_some());
    assert_eq!(result.indexed.len(), 1);
    assert!(result.results.is_empty());
    assert!(matches!(
        service.search(&actor, "Unindexed show").await,
        Err(Error::RateLimited { .. })
    ));
}

#[tokio::test]
async fn viewing_signals_rank_above_recaps_without_approving_playback() {
    let (_dir, service, http, actor) = fixture().await;
    http.search(json!([
      {"url":"https://example.org/news","title":"Some show news recap"},
      {"url":"https://example.org/watch","title":"Some show watch episodes"}
    ]));
    let result = service.search(&actor, "Some show").await.unwrap();
    assert_eq!(result.results[0].url, "https://example.org/watch");
    assert!(result.results.iter().all(|s| s.availability == Availability::Unchecked));
}

#[tokio::test]
async fn invalid_provider_configuration_is_rejected_before_persistence() {
    let (_dir, service, _http, _actor) = fixture().await;
    for settings in [
        json!({"searchProvider":"unknown"}),
        json!({"searchProvider":"searxng","searxngUrl":"http://127.0.0.1"}),
        json!({"searchProvider":"searxng","searxngUrl":"https://example.org/?token=secret"}),
    ] {
        assert!(matches!(
            service.plugins.update_config("webdrama", true, settings).await,
            Err(Error::Validation(_))
        ));
    }
    assert_eq!(service.plugins.get_config("webdrama").await.unwrap().settings, json!({}));
}

#[test]
fn episode_main_entity_wins_over_trailer_and_related_episode_media() {
    let mut result = source("https://example.org/episode");
    let base = Url::parse(&result.url).unwrap();
    let html = r##"<script type="application/ld+json">{"@graph":[{"@type":"VideoObject","name":"Trailer","contentUrl":"/trailer.mp4"},{"@type":"WebPage","mainEntity":{"@type":"TVEpisode","name":"Correct episode","url":"/episode","episodeNumber":4,"hasPart":{"@type":"TVEpisode","name":"Other episode","contentUrl":"/other.mp4"},"associatedMedia":{"@type":"VideoObject","contentUrl":"/correct.mp4"}}}]}</script>"##;
    assert_eq!(
        metadata::extract(&mut result, &base, html).as_deref(),
        Some("https://example.org/correct.mp4")
    );
    assert_eq!(result.title, "Correct episode");
    assert_eq!(result.episode_number, Some(4));
    assert!(result.episodes.is_empty(), "An episode must not become a related-episode catalogue");
}

#[test]
fn related_media_and_series_trailers_never_supply_episode_playback() {
    let mut result = source("https://example.org/episode");
    let base = Url::parse(&result.url).unwrap();
    let html = r#"<script type="application/ld+json">{"@type":"TVEpisode","name":"Current","hasPart":{"@type":"TVEpisode","name":"Other","contentUrl":"/other.mp4"}}</script>"#;
    assert!(metadata::extract(&mut result, &base, html).is_none());
    let mut result = source(base.as_str());
    let html = r#"<script type="application/ld+json">{"@type":"TVSeries","name":"Show"}</script><video src="/trailer.mp4"></video>"#;
    assert!(metadata::extract(&mut result, &base, html).is_none());
}

#[test]
fn html_sources_ignore_comments_scripts_templates_and_ambiguous_players() {
    let base = Url::parse("https://example.org/episode").unwrap();
    let html = r#"<!-- <video src='/comment.mp4'></video> --><script>const x="<video src='/script.mp4'></video>";</script><template><template></template><video src='/template.mp4'></video></template><video controls><source src='/correct.mp4?x=1&amp;y=2'></video>"#;
    let mut result = source(base.as_str());
    assert_eq!(
        metadata::extract(&mut result, &base, html).as_deref(),
        Some("https://example.org/correct.mp4?x=1&y=2")
    );
    for html in [
        r#"<video src='/one.mp4'></video><video src='/two.mp4'></video>"#,
        r#"<video><source src='/one.mp4'><source src='/two.mp4'></video>"#,
        r#"<video src='http://127.0.0.1/private'></video>"#,
        r#"<iframe src='/embed'></iframe>"#,
        r#"<video data-src='/fake.mp4'></video>"#,
    ] {
        assert!(metadata::extract(&mut source(base.as_str()), &base, html).is_none(), "{html}");
    }
}

#[tokio::test]
async fn html_episode_sources_are_decoded_but_hls_is_not_approved() {
    let (_dir, service, http, actor) = fixture().await;
    http.set(
        "https://example.org/html",
        200,
        "text/html",
        b"<title>Episode 4</title><video controls><source src='/clip.mp4'></video>".to_vec(),
    );
    http.set("https://example.org/clip.mp4", 200, "video/mp4", clip());
    let result = service.inspect(&actor, "https://example.org/html").await.unwrap();
    assert_eq!(result.availability, Availability::MediaDetected);
    assert_eq!(result.media_url.as_deref(), Some("https://example.org/clip.mp4"));
    http.set(
        "https://example.org/html",
        200,
        "text/html",
        b"<video src='/stream.m3u8'></video>".to_vec(),
    );
    http.set(
        "https://example.org/stream.m3u8",
        200,
        "application/vnd.apple.mpegurl",
        b"#EXTM3U\nhttp://127.0.0.1/private.ts\n".to_vec(),
    );
    let result = service.inspect(&actor, "https://example.org/html").await.unwrap();
    assert_eq!(result.availability, Availability::ExternalPlayer);
    assert!(result.media_url.is_none());
    assert_eq!(http.calls.load(Ordering::SeqCst), 4);
}

#[test]
fn graph_main_entity_references_and_script_type_are_respected() {
    let base = Url::parse("https://example.org/watch").unwrap();
    let mut result = source(base.as_str());
    let html = r##"<!-- <script type='application/ld+json'>{"@type":"TVEpisode","name":"Comment","contentUrl":"/wrong.mp4"}</script> --><script data-type='application/ld+json'>{"@type":"TVEpisode","name":"Wrong attribute","contentUrl":"/wrong.mp4"}</script><script type='application/ld+json'>{"@graph":[{"@type":"Movie","name":"Actual","@id":"#actual","contentUrl":"/actual.mp4"},{"@type":"TVEpisode","name":"Unrelated","contentUrl":"/wrong.mp4"},{"@type":"WebPage","url":"/watch","mainEntity":{"@id":"#actual"}}]}</script>"##;
    assert_eq!(
        metadata::extract(&mut result, &base, html).as_deref(),
        Some("https://example.org/actual.mp4")
    );
    assert_eq!(result.title, "Actual");
}

async fn login(service: &OnlineService, actor: &Actor, seed: u8) -> crate::Authenticated {
    let now = mbl_core::time::now_ms();
    let id = mbl_db::repo::sessions::SessionsRepo::new(service.db.clone())
        .insert(&mbl_db::repo::sessions::NewSession {
            user_id: actor.user_id,
            token_hash: &[seed; 32],
            device_name: None,
            client_name: None,
            now,
            expires_at: now + 600000,
        })
        .await
        .unwrap();
    crate::Authenticated { actor: actor.clone(), session_id: id }
}
fn ts_clip() -> Vec<u8> {
    static TS: OnceLock<Vec<u8>> = OnceLock::new();
    TS.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.mp4");
        let output = dir.path().join("part.ts");
        std::fs::write(&input, clip()).unwrap();
        assert!(
            std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-y", "-i"])
                .arg(input)
                .args(["-c", "copy", "-f", "mpegts"])
                .arg(&output)
                .status()
                .unwrap()
                .success()
        );
        std::fs::read(output).unwrap()
    })
    .clone()
}
#[test]
fn vod_hls_rewrites_all_resources_and_rejects_unsafe_or_unsupported_manifests() {
    let base = Url::parse("https://example.org/show/index.m3u8").unwrap();
    let valid = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:1,\npart.m4s\n#EXT-X-ENDLIST\n";
    let ranged_map = valid.replace("init.mp4\"", "init.mp4\",BYTERANGE=\"10@0\"");
    assert!(hls::parse(&base, &ranged_map).is_err());
    let playlist = hls::parse(&base, valid).unwrap();
    assert_eq!(
        playlist.urls,
        vec!["https://example.org/show/init.mp4", "https://example.org/show/part.m4s"]
    );
    assert!(playlist.lines.iter().any(|line| line == "resource/1"));
    for body in [
        valid.replace("part.m4s", "http://127.0.0.1/private"),
        valid.replace("#EXT-X-ENDLIST", ""),
        valid.replace("#EXTINF:1,", "#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n#EXTINF:1,"),
        valid.replace("#EXTINF:1,", "#EXT-X-BYTERANGE:10@0\n#EXTINF:1,"),
        valid.replace("#EXTINF:1,", "#EXTINF:NaN,"),
    ] {
        assert!(hls::parse(&base, &body).is_err(), "{body}");
    }
    assert!(hls::variant(&base,"#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=100\nv.m3u8\n#EXT-X-MEDIA:TYPE=AUDIO,URI=\"audio.m3u8\"").is_err());
}
#[tokio::test]
async fn real_hls_relay_is_login_bound_and_resume_survives_session_recreation() {
    let (_dir, service, http, actor) = fixture().await;
    let auth = login(&service, &actor, 1).await;
    http.set(
        "https://example.org/show.m3u8",
        200,
        "application/vnd.apple.mpegurl",
        b"#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:1,\npart.ts\n#EXT-X-ENDLIST\n".to_vec(),
    );
    http.set("https://example.org/part.ts", 200, "video/mp2t", ts_clip());
    let playback = service.start_online(&auth, "https://example.org/show.m3u8").await.unwrap();
    assert!(playback.hls);
    service
        .online_progress(
            &auth,
            &playback.id,
            mbl_core::online::OnlineProgress {
                position: 0.0,
                duration: 1.0,
                ended: false,
                heartbeat_only: true,
            },
        )
        .await
        .unwrap();
    assert!(service.online_history(&actor).await.unwrap().is_empty());
    assert!(
        !service.online_manifest(&auth, &playback.id, None).await.unwrap().contains("https://")
    );
    let manifest_with_tok =
        service.online_manifest(&auth, &playback.id, Some("cast-tok")).await.unwrap();
    assert!(manifest_with_tok.contains("resource/0?token=cast-tok"));
    assert_eq!(service.online_resource(&auth, &playback.id, 0).await.unwrap(), ts_clip());
    let foreign = login(&service, &actor, 2).await;
    assert!(matches!(
        service.online_resource(&foreign, &playback.id, 0).await,
        Err(Error::NotFound)
    ));
    service
        .online_progress(
            &auth,
            &playback.id,
            mbl_core::online::OnlineProgress {
                position: 0.5,
                duration: 1.0,
                ended: false,
                heartbeat_only: false,
            },
        )
        .await
        .unwrap();
    service.stop_online(&auth, &playback.id).await.unwrap();
    assert!(service.online_manifest(&auth, &playback.id, None).await.is_err());
    let playback = service.start_online(&auth, "https://example.org/show.m3u8").await.unwrap();
    assert_eq!(playback.position, 0.5);
    assert_eq!(service.online_history(&actor).await.unwrap()[0].position, 0.5);
    service
        .online_progress(
            &auth,
            &playback.id,
            mbl_core::online::OnlineProgress {
                position: 1.0,
                duration: 1.0,
                ended: true,
                heartbeat_only: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(service.online_history(&actor).await.unwrap()[0].position, 0.0);
    mbl_db::repo::m6::update_user_permissions(&service.db, actor.user_id, Some(true), None, None)
        .await
        .unwrap();
    assert!(matches!(service.online_resource(&auth, &playback.id, 0).await, Err(Error::Forbidden)));
    assert!(matches!(service.online_history(&actor).await, Err(Error::Forbidden)));
}
#[tokio::test]
async fn revoked_login_or_disabled_plugin_cannot_reuse_online_sessions() {
    let (_dir, service, http, actor) = fixture().await;
    let auth = login(&service, &actor, 4).await;
    http.set("https://example.org/direct.mp4", 200, "video/mp4", clip());
    let playback = service.start_online(&auth, "https://example.org/direct.mp4").await.unwrap();
    let repo = mbl_db::repo::sessions::SessionsRepo::new(service.db.clone());
    repo.delete(auth.session_id).await.unwrap();
    assert!(service.stop_online(&auth, &playback.id).await.is_err());
    let other = login(&service, &actor, 5).await;
    assert!(service.stop_online(&other, &playback.id).await.is_err());
    let playback = service.start_online(&other, "https://example.org/direct.mp4").await.unwrap();
    service.plugins.update_config("webdrama", false, json!({})).await.unwrap();
    assert!(
        service
            .online_progress(
                &other,
                &playback.id,
                mbl_core::online::OnlineProgress {
                    position: 0.0,
                    duration: 1.0,
                    ended: false,
                    heartbeat_only: false
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_changed_episode_on_the_same_page_does_not_inherit_old_resume() {
    let (_dir, service, http, actor) = fixture().await;
    let auth = login(&service, &actor, 8).await;
    let url = "https://example.org/episode";
    http.set(
        url,
        200,
        "text/html",
        b"<title>Episode one</title><video src='/clip.mp4'></video>".to_vec(),
    );
    http.set("https://example.org/clip.mp4", 200, "video/mp4", clip());
    let play = service.start_online(&auth, url).await.unwrap();
    service
        .online_progress(
            &auth,
            &play.id,
            mbl_core::online::OnlineProgress {
                position: 0.5,
                duration: 1.0,
                ended: false,
                heartbeat_only: false,
            },
        )
        .await
        .unwrap();
    service.stop_online(&auth, &play.id).await.unwrap();
    http.set(
        url,
        200,
        "text/html",
        b"<title>Episode two</title><video src='/clip.mp4'></video>".to_vec(),
    );
    assert_eq!(service.start_online(&auth, url).await.unwrap().position, 0.0);
}
