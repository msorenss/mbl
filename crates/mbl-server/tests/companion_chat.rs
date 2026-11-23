//! Co-watching chat: viewer messages, comment modes and dialogue from subtitles.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::{Value, json};

const SRT: &str = "1\n00:00:40,000 --> 00:00:41,500\nAre you happy with your life?\n\n2\n00:00:41,800 --> 00:00:43,000\n<i>Yes.</i>\n\n3\n00:00:45,000 --> 00:00:47,000\nThis line comes later.\n";

async fn context(app: &TestApp, token: &str, session: &str) -> Value {
    // Extraction runs in the background; the first answers say "preparing".
    for _ in 0..100 {
        let res = app
            .call(
                Method::GET,
                &format!("/api/v1/companion/sessions/{session}/context"),
                Some(token),
                None,
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "{}", res.body);
        if res.body["dialogueStatus"] != "preparing" {
            return res.body;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("dialogue never became ready");
}

#[tokio::test]
async fn viewer_chats_with_companions_and_controls_what_they_say() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;

    // A real external SRT next to the movie, as the scanner would record it.
    let movies_dir = app.media_dir("Movies");
    let srt = std::path::Path::new(&movies_dir).join("Heat (1995).en.srt");
    std::fs::write(&srt, SRT).unwrap();
    sqlx::query(
        "INSERT INTO streams (part_id, idx, kind, codec, language, is_default, external_path)
         VALUES (?, 2, 'subtitle', 'subrip', 'eng', 1, ?)",
    )
    .bind(fixture.movie_part_id)
    .bind(srt.to_str().unwrap())
    .execute(&db.write)
    .await
    .unwrap();

    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId": fixture.movie_id, "profile": "chrome", "startMs": 42000})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    let session = start.body["sessionId"].as_str().unwrap().to_owned();
    let base = format!("/api/v1/companion/sessions/{session}");

    // Posting before joining says what to do instead of a bare 404.
    let res = app
        .call(
            Method::POST,
            &format!("{base}/reaction"),
            Some(&admin),
            Some(json!({"botName": "Claude", "timestampMs": 42000, "commentary": "Hi"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT, "{}", res.body);
    assert!(res.body.to_string().contains("join"), "{}", res.body);

    let res = app
        .call(
            Method::POST,
            &format!("{base}/join"),
            Some(&admin),
            Some(json!({"botName": "Claude"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    // The viewer's name is reserved.
    let res = app
        .call(
            Method::POST,
            &format!("{base}/join"),
            Some(&admin),
            Some(json!({"botName": "viewer"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Dialogue up to the position, never after it.
    let ctx = context(&app, &admin, &session).await;
    assert_eq!(ctx["dialogueStatus"], "ready", "{ctx}");
    assert_eq!(ctx["dialogueStreamIndex"], 2);
    assert_eq!(ctx["currentSubtitleCue"], "Yes.");
    let lines: Vec<&str> = ctx["recentDialogue"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap())
        .collect();
    assert_eq!(lines, ["Are you happy with your life?", "Yes."]);
    assert_eq!(ctx["mode"], "on");

    // The client reports the shown track; an unknown index falls back to the default.
    let res = app
        .call(
            Method::POST,
            &format!("/api/v1/playback/{session}/progress"),
            Some(&admin),
            Some(json!({"positionMs": 46000, "state": "playing", "subtitleStreamIndex": 9})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT, "{}", res.body);
    let ctx = context(&app, &admin, &session).await;
    assert_eq!(ctx["dialogueStreamIndex"], 2);
    assert_eq!(ctx["currentSubtitleCue"], "This line comes later.");

    // The viewer writes; the message carries the position and an ID.
    let res = app
        .call(
            Method::POST,
            &format!("{base}/messages"),
            Some(&admin),
            Some(json!({"text": "  Who is that?  "})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["fromViewer"], true);
    assert_eq!(res.body["commentary"], "Who is that?");
    assert_eq!(res.body["timestampMs"], 46000);
    let question = res.body["id"].as_u64().unwrap();
    for bad in ["   ", &"x".repeat(501)] {
        let res = app
            .call(
                Method::POST,
                &format!("{base}/messages"),
                Some(&admin),
                Some(json!({"text": bad})),
            )
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Replies only: a free comment is refused, an answer is accepted.
    let res = app
        .call(Method::PUT, &format!("{base}/mode"), Some(&admin), Some(json!({"mode": "replies"})))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let comment = |reply_to: Option<u64>| json!({"botName": "Claude", "timestampMs": 46000, "commentary": "Hi", "replyTo": reply_to});
    let res = app
        .call(Method::POST, &format!("{base}/reaction"), Some(&admin), Some(comment(None)))
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT, "{}", res.body);
    let res = app
        .call(Method::POST, &format!("{base}/reaction"), Some(&admin), Some(comment(Some(9999))))
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let res = app
        .call(
            Method::POST,
            &format!("{base}/reaction"),
            Some(&admin),
            Some(comment(Some(question))),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["replyTo"], question);

    // Off: nothing may be posted, not even answers.
    let res = app
        .call(Method::PUT, &format!("{base}/mode"), Some(&admin), Some(json!({"mode": "off"})))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let res = app
        .call(
            Method::POST,
            &format!("{base}/reaction"),
            Some(&admin),
            Some(comment(Some(question))),
        )
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);

    // The feed shows the conversation in order and can be read from an ID.
    let res = app.call(Method::GET, &format!("{base}/reactions"), Some(&admin), None).await;
    assert_eq!(res.body["mode"], "off");
    let entries = res.body["reactions"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["fromViewer"], true);
    assert_eq!(entries[1]["botName"], "Claude");
    let res = app
        .call(Method::GET, &format!("{base}/reactions?afterId={question}"), Some(&admin), None)
        .await;
    assert_eq!(res.body["reactions"].as_array().unwrap().len(), 1);

    // Someone else cannot read, write to or reconfigure this session.
    let other = app.create_user(&admin, "other").await;
    for (method, path, body) in [
        (Method::GET, format!("{base}/reactions"), None),
        (Method::POST, format!("{base}/messages"), Some(json!({"text": "hi"}))),
        (Method::PUT, format!("{base}/mode"), Some(json!({"mode": "on"}))),
    ] {
        let res = app.call(method, &path, Some(&other), body).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn companions_wake_on_viewer_messages_and_requested_comments() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;
    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId": fixture.movie_id, "profile": "chrome", "startMs": 0})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    let session = start.body["sessionId"].as_str().unwrap().to_owned();
    let base = format!("/api/v1/companion/sessions/{session}");

    // Waiting requires joining first.
    let res =
        app.call(Method::GET, &format!("{base}/wait?botName=Claude"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::CONFLICT, "{}", res.body);
    let res = app
        .call(
            Method::POST,
            &format!("{base}/join"),
            Some(&admin),
            Some(json!({"botName": "Claude"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);

    // Nothing happens: the wait ends after the timeout, and the companion shows as listening.
    let res = app
        .call(Method::GET, &format!("{base}/wait?botName=Claude&timeoutMs=200"), Some(&admin), None)
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["timedOut"], true);
    assert_eq!(res.body["commentDue"], Value::Null);
    let after = res.body["lastId"].as_u64().unwrap();
    let res = app.call(Method::GET, &format!("{base}/reactions"), Some(&admin), None).await;
    assert_eq!(res.body["listening"], json!(["Claude"]));
    assert_eq!(res.body["frequency"], "medium");

    // A message written while the companion waits wakes it at once.
    let wait_path = format!("{base}/wait?botName=Claude&afterId={after}&timeoutMs=20000");
    let started = std::time::Instant::now();
    let (woken, sent) =
        tokio::join!(app.call(Method::GET, &wait_path, Some(&admin), None), async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            app.call(
                Method::POST,
                &format!("{base}/messages"),
                Some(&admin),
                Some(json!({"text": "Where did they go?"})),
            )
            .await
        });
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
    assert_eq!(woken.status, StatusCode::OK, "{}", woken.body);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(woken.body["timedOut"], false);
    assert_eq!(woken.body["viewerMessages"][0]["commentary"], "Where did they go?");
    assert_eq!(woken.body["context"]["sessionId"], session.as_str());
    let after = woken.body["lastId"].as_u64().unwrap();

    // "Comment now" in replies-only mode allows one comment without reply_to.
    let res = app
        .call(
            Method::PUT,
            &format!("{base}/mode"),
            Some(&admin),
            Some(json!({"mode": "replies", "frequency": "high"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let reaction = json!({"botName": "Claude", "timestampMs": 0, "commentary": "Unprompted"});
    let res = app
        .call(Method::POST, &format!("{base}/reaction"), Some(&admin), Some(reaction.clone()))
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    let res = app.call(Method::POST, &format!("{base}/nudge"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let res = app
        .call(
            Method::GET,
            &format!("{base}/wait?botName=Claude&afterId={after}&timeoutMs=2000"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(res.body["commentDue"], "requested", "{}", res.body);
    assert_eq!(res.body["context"]["frequency"], "high");
    let res = app
        .call(Method::POST, &format!("{base}/reaction"), Some(&admin), Some(reaction.clone()))
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    // Only one.
    let res =
        app.call(Method::POST, &format!("{base}/reaction"), Some(&admin), Some(reaction)).await;
    assert_eq!(res.status, StatusCode::CONFLICT);

    // With comments off the viewer cannot ask, and someone else cannot ask at all.
    let res = app
        .call(Method::PUT, &format!("{base}/mode"), Some(&admin), Some(json!({"mode": "off"})))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let res = app.call(Method::POST, &format!("{base}/nudge"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    let other = app.create_user(&admin, "other").await;
    let res = app.call(Method::POST, &format!("{base}/nudge"), Some(&other), None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res =
        app.call(Method::GET, &format!("{base}/wait?botName=Claude"), Some(&other), None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_cast_receiver_reads_comments_with_its_own_token() {
    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;
    let start = app
        .call(
            Method::POST,
            "/api/v1/playback",
            Some(&admin),
            Some(json!({"itemId": fixture.movie_id, "profile": "chrome", "startMs": 0})),
        )
        .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.body);
    let session = start.body["sessionId"].as_str().unwrap().to_owned();
    let base = format!("/api/v1/companion/sessions/{session}");
    let res = app
        .call(Method::POST, &format!("{base}/join"), Some(&admin), Some(json!({"botName": "MBL"})))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let res = app
        .call(
            Method::POST,
            &format!("{base}/reaction"),
            Some(&admin),
            Some(json!({"botName": "MBL", "timestampMs": 1000, "commentary": "Watch the door"})),
        )
        .await;
    assert!(res.status.is_success(), "{}", res.body);
    let res = app
        .call(Method::POST, &format!("{base}/messages"), Some(&admin), Some(json!({"text": "hi"})))
        .await;
    let viewer_id = res.body["id"].as_u64().unwrap();

    // Only someone who can see the session gets a token.
    let res = app
        .call(Method::POST, "/api/v1/companion/sessions/nope/receiver-token", Some(&admin), None)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app.call(Method::POST, &format!("{base}/receiver-token"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let token = res.body["token"].as_str().unwrap().to_owned();
    let enc: String = token.bytes().map(|b| format!("%{b:02X}")).collect();

    // The receiver reads the feed with the token alone, no sign-in.
    let feed = format!("/api/v1/companion/receiver/{session}/reactions?token={enc}");
    let res = app.call(Method::GET, &feed, None, None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let texts: Vec<&str> = res.body["reactions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["commentary"].as_str())
        .collect();
    assert!(texts.contains(&"Watch the door"), "{texts:?}");
    let comment_id = res.body["reactions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["commentary"] == "Watch the door")
        .and_then(|r| r["id"].as_u64())
        .unwrap();

    // A wrong token, or this token for another session, is refused.
    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/companion/receiver/{session}/reactions?token=x.y"),
            None,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/companion/receiver/other/reactions?token={enc}"),
            None,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);

    // Reading aloud: only companion comments, never the viewer's own words or arbitrary text.
    let speech = format!("/api/v1/companion/receiver/{session}/speech");
    let res = app
        .call(Method::GET, &format!("{speech}/{viewer_id}?token={enc}&language=sv"), None, None)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app
        .call(Method::GET, &format!("{speech}/{comment_id}?token={enc}&language=sv"), None, None)
        .await;
    // No Piper in the test environment: the comment exists, the voice does not.
    assert!(
        matches!(res.status, StatusCode::SERVICE_UNAVAILABLE | StatusCode::OK),
        "{} {}",
        res.status,
        res.body
    );
}
