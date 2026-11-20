//! Built-in assistant settings: admin only, key never returned, connection test and model list
//! against a fake OpenAI-compatible provider.

#![allow(clippy::unwrap_used)]

mod common;

use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use common::TestApp;
use serde_json::{Value, json};

async fn fake_provider() -> String {
    async fn chat(headers: HeaderMap, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
        if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer sk-test") {
            return (StatusCode::UNAUTHORIZED, Json(json!({"error": {"message": "bad key"}})));
        }
        if body["tools"].is_array() {
            return (
                StatusCode::OK,
                Json(
                    json!({"model": "tiny", "choices": [{"finish_reason": "tool_calls", "message": {
                        "content": null,
                        "tool_calls": [{"id": "c", "function": {"name": "get_time", "arguments": "{}"}}]
                    }}]}),
                ),
            );
        }
        (
            StatusCode::OK,
            Json(
                json!({"model": "tiny", "choices": [{"finish_reason": "stop", "message": {"content": "OK"}}]}),
            ),
        )
    }
    async fn models() -> Json<Value> {
        Json(json!({"data": [{"id": "tiny"}, {"id": "big"}]}))
    }
    let app =
        Router::new().route("/v1/chat/completions", post(chat)).route("/v1/models", get(models));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn admin_configures_and_tests_the_assistant() {
    let (app, _db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let base = fake_provider().await;

    let res = app.call(Method::GET, "/api/v1/system/assistant", Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["enabled"], false);
    assert_eq!(res.body["provider"], "openai_compatible");

    // Turning it on needs an address and a model.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": "", "model": ""})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": base, "model": "tiny", "effort": "turbo"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);

    // Models and the connection test use the key from the form before it is saved.
    let connection = json!({"provider": "openai_compatible", "baseUrl": base, "apiKey": "sk-test", "model": "tiny"});
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/models",
            Some(&admin),
            Some(connection.clone()),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body, json!(["big", "tiny"]));
    let res = app
        .call(Method::POST, "/api/v1/system/assistant/test", Some(&admin), Some(connection))
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["ok"], true, "{}", res.body);
    assert_eq!(res.body["reply"], "OK");
    assert_eq!(res.body["toolsSupported"], true);

    // Save; the key is stored but never returned.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": format!("{base}/"), "apiKey": "sk-test", "model": "tiny", "effort": "Low"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["hasApiKey"], true);
    assert_eq!(res.body["baseUrl"], base);
    assert_eq!(res.body["effort"], "low");
    assert!(!res.body.to_string().contains("sk-test"));

    // Without a key in the form the stored one is used; a wrong key is reported, not thrown.
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(&admin),
            Some(json!({"provider": "openai_compatible", "baseUrl": base, "model": "tiny"})),
        )
        .await;
    assert_eq!(res.body["ok"], true, "{}", res.body);
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(&admin),
            Some(json!({"provider": "openai_compatible", "baseUrl": base, "apiKey": "wrong", "model": "tiny"})),
        )
        .await;
    assert_eq!(res.body["ok"], false);
    assert!(res.body["error"].as_str().unwrap().contains("rejected"), "{}", res.body);

    // The saved provider is ready for the assistant to use.
    let provider = services.assistant.provider().await.unwrap().expect("turned on");
    assert_eq!(provider.config().model, "tiny");
    assert_eq!(provider.config().api_key.as_deref(), Some("sk-test"));

    // Clearing the key works for local servers.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": false, "provider": "anthropic", "baseUrl": "https://api.anthropic.com/v1", "clearApiKey": true, "model": "claude-opus-5-5"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["hasApiKey"], false);
    assert_eq!(res.body["baseUrl"], "https://api.anthropic.com");
    assert!(services.assistant.provider().await.unwrap().is_none());

    // Other users cannot read or change it.
    let user = app.create_user(&admin, "viewer").await;
    for (method, path, body) in [
        (Method::GET, "/api/v1/system/assistant", None),
        (
            Method::PUT,
            "/api/v1/system/assistant",
            Some(json!({"enabled": false, "provider": "anthropic", "baseUrl": "", "model": ""})),
        ),
        (
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(json!({"provider": "anthropic", "baseUrl": "https://x", "model": "m"})),
        ),
        (
            Method::POST,
            "/api/v1/system/assistant/models",
            Some(json!({"provider": "anthropic", "baseUrl": "https://x"})),
        ),
    ] {
        let res = app.call(method, path, Some(&user), body).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}: {}", res.body);
    }
}

/// A provider that first asks for `find_by_person`, then answers with what the tool returned.
async fn scripted_provider(seen: std::sync::Arc<std::sync::Mutex<Vec<Value>>>) -> String {
    async fn chat(
        axum::extract::State(seen): axum::extract::State<
            std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
        >,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        seen.lock().unwrap().push(body.clone());
        let last = body["messages"].as_array().unwrap().last().unwrap().clone();
        if last["role"] == "tool" {
            let content = last["content"].as_str().unwrap_or_default().to_owned();
            return Json(
                json!({"choices": [{"finish_reason": "stop", "message": {"content": format!("Tool said: {content}")}}],
                "usage": {"prompt_tokens": 50, "completion_tokens": 10}}),
            );
        }
        // "play <id>" asks to start that item here.
        let asked = last["content"].as_str().unwrap_or_default();
        let call = match asked.strip_prefix("play ") {
            Some(id) => json!({"id": "c1", "type": "function", "function": {
                "name": "play_here", "arguments": format!("{{\"item_id\":{id}}}")}}),
            None => json!({"id": "c1", "type": "function", "function": {
                "name": "find_by_person", "arguments": "{\"name\":\"pacino\"}"}}),
        };
        Json(json!({"choices": [{"finish_reason": "tool_calls", "message": {"content": null,
            "tool_calls": [call]}}], "usage": {"prompt_tokens": 40, "completion_tokens": 5}}))
    }
    let app = Router::new().route("/v1/chat/completions", post(chat)).with_state(seen);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn the_assistant_looks_things_up_as_the_person_asking() {
    use mbl_core::{Actor, LibraryAccess, UserId};
    use mbl_mcp::assistant::{AssistantRunner, Surface};

    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;
    let person: i64 =
        sqlx::query_scalar("INSERT INTO people (name) VALUES ('Al Pacino') RETURNING id")
            .fetch_one(&db.write)
            .await
            .unwrap();
    sqlx::query("INSERT INTO item_people (item_id, person_id, role, character, ord) VALUES (?, ?, 'actor', 'Vincent Hanna', 0)")
        .bind(fixture.movie_id)
        .bind(person)
        .execute(&db.write)
        .await
        .unwrap();
    let admin_id: i64 =
        sqlx::query_scalar("SELECT MIN(id) FROM users").fetch_one(&db.read).await.unwrap();
    let admin_actor = Actor::admin(UserId(admin_id));
    let runner = AssistantRunner::new(db.clone(), services.clone());
    let ask = || vec![mbl_agent::Message::user("Anything with Al Pacino?")];

    // Off until configured.
    let e = runner.run(&admin_actor, Surface::Chat, "", ask(), None).await.unwrap_err();
    assert!(matches!(e, mbl_core::Error::Unavailable(_)), "{e}");

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let base = scripted_provider(seen.clone()).await;
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": base, "model": "scripted"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);

    // This fake ignores `stream` and answers with plain JSON; the text still arrives.
    let mut streamed = String::new();
    let mut sink = |s: &str| streamed.push_str(s);
    let reply = runner.run(&admin_actor, Surface::Chat, "", ask(), Some(&mut sink)).await.unwrap();
    assert_eq!(streamed, reply.text);
    assert!(reply.text.contains("Heat"), "{}", reply.text);
    assert!(reply.text.contains("Vincent Hanna"), "{}", reply.text);
    // The movie the tool found and the answer names becomes a link.
    assert_eq!(reply.items.len(), 1, "{:?}", reply.items);
    assert_eq!((reply.items[0].id, reply.items[0].title.as_str()), (fixture.movie_id, "Heat"));
    assert_eq!(reply.steps, 2);
    assert_eq!(reply.tools.len(), 1);
    assert!(reply.tools[0].ok);
    assert_eq!((reply.usage.input_tokens, reply.usage.output_tokens), (90, 15));

    // The model saw the rules and only the chat tools; nothing that changes data.
    {
        let bodies = seen.lock().unwrap();
        let first = &bodies[0];
        let system = first["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains("data, not instructions"), "{system}");
        let names: Vec<&str> = first["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"find_by_person") && names.contains(&"search_media"), "{names:?}");
        // The default role reads and plays; it cannot scan, test hardware or manage users.
        assert!(names.contains(&"play_here") && names.contains(&"play_on_player"), "{names:?}");
        assert!(!names.iter().any(|n| {
            ["scan_library", "run_hw_selftest", "delete_user", "create_user"].contains(n)
        }));
    }

    // Starting a title here: the item comes back for the browser to play.
    let reply = runner
        .run(
            &admin_actor,
            Surface::Chat,
            "",
            vec![mbl_agent::Message::user(format!("play {}", fixture.movie_id))],
            None,
        )
        .await
        .unwrap();
    let play = reply.play.expect("a title to play");
    assert_eq!((play.id, play.title.as_str()), (fixture.movie_id, "Heat"));
    let reply = runner
        .run(&admin_actor, Surface::Chat, "", vec![mbl_agent::Message::user("play 999999")], None)
        .await
        .unwrap();
    assert!(reply.play.is_none(), "an unknown item is not played");

    // Roles: other users can never be given admin access.
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": base, "model": "scripted", "userScopes": ["read", "admin"]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", res.body);
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": base, "model": "scripted", "adminScopes": ["admin"], "userScopes": ["library"]})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["adminScopes"], json!(["read", "admin"]));
    assert_eq!(res.body["userScopes"], json!(["read", "library"]));
    let tool_names = |body: &Value| -> Vec<String> {
        body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_owned())
            .collect()
    };
    runner.run(&admin_actor, Surface::Chat, "", ask(), None).await.unwrap();
    let names = tool_names(seen.lock().unwrap().iter().rev().nth(1).unwrap());
    assert!(names.iter().any(|n| n == "run_hw_selftest"), "admins may now test hardware");
    // The companion stays read-only whatever the role.
    let companion_names = {
        runner.run(&admin_actor, Surface::Companion, "", ask(), None).await.unwrap();
        tool_names(seen.lock().unwrap().iter().rev().nth(1).unwrap())
    };
    assert!(!companion_names.iter().any(|n| n == "run_hw_selftest" || n == "play_here"));

    // The tool call is audited as the assistant.
    let audited: (String, String, String) =
        sqlx::query_as("SELECT client, tool, outcome FROM mcp_audit ORDER BY id DESC LIMIT 1")
            .fetch_one(&db.read)
            .await
            .unwrap();
    assert_eq!(audited, ("mbl-assistant".into(), "find_by_person".into(), "ok".into()));

    // Someone without access to the movie library gets nothing from the same question.
    let viewer = Actor {
        user_id: UserId(admin_id + 100),
        is_admin: false,
        library_access: LibraryAccess::Only(Vec::new()),
        is_managed: false,
        content_rating_ceiling: None,
    };
    let reply = runner.run(&viewer, Surface::Chat, "", ask(), None).await.unwrap();
    assert!(!reply.text.contains("Heat"), "{}", reply.text);
    assert!(reply.text.contains("\"count\":0"), "{}", reply.text);
    // The user role (read and library) gives no admin tools and, without playback, no playing.
    let names = tool_names(seen.lock().unwrap().iter().rev().nth(1).unwrap());
    assert!(
        !names.iter().any(|n| n == "run_hw_selftest" || n == "delete_user" || n == "play_here")
    );

    // Managed (child) profiles do not get the assistant.
    let child = Actor { is_managed: true, ..viewer };
    let e = runner.run(&child, Surface::Chat, "", ask(), None).await.unwrap_err();
    assert!(matches!(e, mbl_core::Error::Forbidden));
}

/// Answers with the last thing the viewer (or the prompt) said, so the test can see which.
async fn echo_provider() -> String {
    async fn chat(Json(body): Json<Value>) -> Json<Value> {
        let messages = body["messages"].as_array().unwrap();
        let system = messages[0]["content"].as_str().unwrap_or_default();
        let last = messages.last().unwrap()["content"].as_str().unwrap_or_default();
        let scene = if system.contains("Position") { "scene known" } else { "no scene" };
        Json(
            json!({"choices": [{"finish_reason": "stop", "message": {"content": format!("Echo ({scene}): {last}")}}]}),
        )
    }
    let app = Router::new().route("/v1/chat/completions", post(chat));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/v1")
}

/// Polls the feed until an entry from the built-in companion matches.
async fn bot_post(
    app: &TestApp,
    token: &str,
    base: &str,
    matches: impl Fn(&Value) -> bool,
) -> Value {
    for _ in 0..100 {
        let res = app.call(Method::GET, &format!("{base}/reactions"), Some(token), None).await;
        if let Some(found) = res.body["reactions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["botName"] == "MBL" && matches(r))
        {
            return found.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the built-in companion did not post");
}

#[tokio::test]
async fn the_built_in_companion_answers_in_the_player_chat() {
    let (app, db, services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let fixture = common::seed_playback_fixtures(&app, &db, &admin).await;
    let base_url = echo_provider().await;
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": base_url, "model": "echo"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let stop = tokio_util::sync::CancellationToken::new();
    tokio::spawn(mbl_mcp::companion_bot::run(
        mbl_mcp::assistant::AssistantRunner::new(db.clone(), services.clone()),
        services.clone(),
        db.clone(),
        stop.clone(),
    ));

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

    // Writing in the chat brings the companion in; it answers that message.
    let sent = app
        .call(
            Method::POST,
            &format!("{base}/messages"),
            Some(&admin),
            Some(json!({"text": "Who is the detective?"})),
        )
        .await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
    let question = sent.body["id"].as_u64().unwrap();
    let answer = bot_post(&app, &admin, &base, |r| r["replyTo"] == question).await;
    assert_eq!(answer["commentary"], "Echo (scene known): Who is the detective?");
    let feed = app.call(Method::GET, &format!("{base}/reactions"), Some(&admin), None).await;
    assert!(feed.body["companions"].as_array().unwrap().contains(&json!("MBL")));

    // "Comment now" works even when the viewer only wants answers.
    let res = app
        .call(Method::PUT, &format!("{base}/mode"), Some(&admin), Some(json!({"mode": "replies"})))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let res = app.call(Method::POST, &format!("{base}/nudge"), Some(&admin), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let comment = bot_post(&app, &admin, &base, |r| {
        r["replyTo"].is_null() && r["commentary"].as_str().unwrap().contains("Comment now")
    })
    .await;
    assert!(comment["commentary"].as_str().unwrap().starts_with("Echo (scene known)"));

    // With comments off it stays quiet, even when written to.
    let res = app
        .call(Method::PUT, &format!("{base}/mode"), Some(&admin), Some(json!({"mode": "off"})))
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let sent = app
        .call(
            Method::POST,
            &format!("{base}/messages"),
            Some(&admin),
            Some(json!({"text": "Hello?"})),
        )
        .await;
    let quiet = sent.body["id"].as_u64().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let feed = app.call(Method::GET, &format!("{base}/reactions"), Some(&admin), None).await;
    assert!(
        !feed.body["reactions"].as_array().unwrap().iter().any(|r| r["replyTo"] == quiet),
        "{}",
        feed.body
    );
    stop.cancel();
}

/// `(event, data)` pairs from a server-sent event body.
fn events(body: &Value) -> Vec<(String, Value)> {
    body.as_str()
        .unwrap_or_default()
        .split("\n\n")
        .filter_map(|block| {
            let mut name = String::new();
            let mut data = String::new();
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    name = v.trim().to_owned();
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push_str(v.trim_start());
                }
            }
            (!name.is_empty()).then(|| (name, serde_json::from_str(&data).unwrap_or(Value::Null)))
        })
        .collect()
}

#[tokio::test]
async fn the_assistant_chat_keeps_private_conversations() {
    let (app, _db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;

    let res = app.call(Method::GET, "/api/v1/assistant/status", Some(&viewer), None).await;
    assert_eq!(res.body, json!({"available": false}));
    let res = app
        .call(Method::POST, "/api/v1/assistant/chat", Some(&viewer), Some(json!({"text": "Hi"})))
        .await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE, "{}", res.body);

    let base_url = echo_provider().await;
    app.call(
        Method::PUT,
        "/api/v1/system/assistant",
        Some(&admin),
        Some(json!({"enabled": true, "provider": "openai_compatible", "baseUrl": base_url, "model": "echo"})),
    )
    .await;
    let res = app.call(Method::GET, "/api/v1/assistant/status", Some(&viewer), None).await;
    assert_eq!(res.body, json!({"available": true}));

    // A new conversation: its id first, then the answer.
    let res = app
        .call(
            Method::POST,
            "/api/v1/assistant/chat",
            Some(&viewer),
            Some(json!({"text": "Something to watch tonight?"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert!(res.headers["content-type"].to_str().unwrap().starts_with("text/event-stream"));
    let stream = events(&res.body);
    assert_eq!(stream.first().unwrap().0, "conversation", "{stream:?}");
    let conversation = stream[0].1["id"].as_i64().unwrap();
    assert_eq!(stream[0].1["title"], "Something to watch tonight?");
    let streamed: String = stream
        .iter()
        .filter(|(name, _)| name == "delta")
        .map(|(_, d)| d["text"].as_str().unwrap().to_owned())
        .collect();
    let (name, done) = stream.last().unwrap();
    assert_eq!(name, "done", "{stream:?}");
    assert_eq!(done["message"]["content"], "Echo (no scene): Something to watch tonight?");
    assert_eq!(streamed, done["message"]["content"].as_str().unwrap());

    // Continuing it sends the earlier turns along.
    let res = app
        .call(
            Method::POST,
            "/api/v1/assistant/chat",
            Some(&viewer),
            Some(json!({"conversationId": conversation, "text": "Something shorter"})),
        )
        .await;
    let stream = events(&res.body);
    assert_eq!(stream[0].1["id"], conversation);
    assert_eq!(
        stream.last().unwrap().1["message"]["content"],
        "Echo (no scene): Something shorter"
    );
    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/assistant/conversations/{conversation}"),
            Some(&viewer),
            None,
        )
        .await;
    let roles: Vec<&str> = res.body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["user", "assistant", "user", "assistant"]);
    let res = app.call(Method::GET, "/api/v1/assistant/conversations", Some(&viewer), None).await;
    assert_eq!(res.body.as_array().unwrap().len(), 1);

    // Conversations are private.
    let res = app
        .call(
            Method::GET,
            &format!("/api/v1/assistant/conversations/{conversation}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app
        .call(
            Method::POST,
            "/api/v1/assistant/chat",
            Some(&admin),
            Some(json!({"conversationId": conversation, "text": "Hijack"})),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app.call(Method::GET, "/api/v1/assistant/conversations", Some(&admin), None).await;
    assert_eq!(res.body, json!([]));

    // Empty and too long messages are refused.
    for text in ["  ", &"x".repeat(2001)] {
        let res = app
            .call(
                Method::POST,
                "/api/v1/assistant/chat",
                Some(&viewer),
                Some(json!({"text": text})),
            )
            .await;
        assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    let res = app
        .call(
            Method::DELETE,
            &format!("/api/v1/assistant/conversations/{conversation}"),
            Some(&viewer),
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    let res = app.call(Method::GET, "/api/v1/assistant/conversations", Some(&viewer), None).await;
    assert_eq!(res.body, json!([]));
}

#[cfg(unix)]
#[tokio::test]
async fn an_agent_program_can_be_the_assistant() {
    use std::os::unix::fs::PermissionsExt;

    let (app, db, _services) = TestApp::new_with_services().await;
    let admin = app.setup_admin().await;
    let viewer = app.create_user(&admin, "viewer").await;
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.txt");
    let script = dir.path().join("agent.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n\
             prompt=$(cat)\n\
             case \"$prompt\" in *'Person: Anything with Al Pacino?'*) ;; *) echo 'no prompt' >&2; exit 3;; esac\n\
             test -n \"$MBL_MCP_TOKEN\" || exit 4\n\
             grep -q \"Bearer $MBL_MCP_TOKEN\" \"$1\" || exit 5\n\
             case \"$*\" in *\"$MBL_MCP_TOKEN\"*) exit 6;; esac\n\
             echo \"$1\" > {seen}\n\
             echo 'Heat is in your library.'\n",
            seen = seen.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let command = format!("{} {{mcp_config}}", script.display());

    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "command", "baseUrl": "", "model": "", "command": ""})),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY, "a command is required");
    let res = app
        .call(
            Method::PUT,
            "/api/v1/system/assistant",
            Some(&admin),
            Some(json!({"enabled": true, "provider": "command", "baseUrl": "", "model": "", "command": command})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["provider"], "command");
    assert_eq!(res.body["command"], command);

    let res = app
        .call(
            Method::POST,
            "/api/v1/assistant/chat",
            Some(&viewer),
            Some(json!({"text": "Anything with Al Pacino?"})),
        )
        .await;
    let stream = events(&res.body);
    let (name, done) = stream.last().unwrap();
    assert_eq!(name, "done", "{stream:?}");
    assert_eq!(done["message"]["content"], "Heat is in your library.");

    // The temporary token is revoked and its configuration file removed.
    let config = std::fs::read_to_string(&seen).unwrap();
    assert!(!std::path::Path::new(config.trim()).exists(), "{config}");
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM api_tokens WHERE name = 'Assistant command (temporary)' AND revoked_at IS NULL",
    )
    .fetch_one(&db.read)
    .await
    .unwrap();
    assert_eq!(open, 0);

    // A failing program is reported, with its last error line.
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(&admin),
            Some(json!({"provider": "command", "baseUrl": "", "command": command})),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert_eq!(res.body["ok"], false);
    assert!(res.body["error"].as_str().unwrap().contains("no prompt"), "{}", res.body);
    let res = app
        .call(
            Method::POST,
            "/api/v1/system/assistant/test",
            Some(&admin),
            Some(json!({"provider": "command", "baseUrl": "", "command": "/no/such/agent"})),
        )
        .await;
    assert!(res.body["error"].as_str().unwrap().contains("Could not start"), "{}", res.body);
}
