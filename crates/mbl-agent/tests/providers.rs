#![allow(clippy::unwrap_used)]

//! Both protocols against a fake provider over real HTTP: plain and streamed answers, tool
//! calls, model lists, the connection test and errors.

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_agent::{
    ChatRequest, Message, Provider, ProviderConfig, ProviderKind, StopReason, ToolSpec,
};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Seen {
    bodies: Arc<Mutex<Vec<Value>>>,
    headers: Arc<Mutex<Vec<HeaderMap>>>,
}

fn sse(events: &[(&str, Value)]) -> Response {
    let mut out = String::new();
    for (name, data) in events {
        if !name.is_empty() {
            out.push_str(&format!("event: {name}\n"));
        }
        match data.as_str() {
            Some(raw) => out.push_str(&format!("data: {raw}\n\n")),
            None => out.push_str(&format!("data: {data}\n\n")),
        }
    }
    ([("content-type", "text/event-stream")], out).into_response()
}

fn has_tools(body: &Value) -> bool {
    body["tools"].as_array().is_some_and(|t| !t.is_empty())
}

async fn openai_chat(
    State(seen): State<Seen>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    seen.bodies.lock().unwrap().push(body.clone());
    seen.headers.lock().unwrap().push(headers.clone());
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer good") {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": {"message": "bad key"}})))
            .into_response();
    }
    if body["model"] == "missing" {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": {"message": "model 'missing' not found"}})),
        )
            .into_response();
    }
    let tools = has_tools(&body);
    if body["stream"] == true {
        let mut events = vec![
            ("", json!({"model": "fake-1", "choices": [{"delta": {"content": "Hel"}}]})),
            ("", json!({"choices": [{"delta": {"content": "lo"}}]})),
        ];
        if tools {
            events.push((
                "",
                json!({"choices": [{"delta": {"tool_calls": [
                    {"index": 0, "id": "call_a", "function": {"name": "find", "arguments": "{\"na"}}
                ]}}]}),
            ));
            events.push((
                "",
                json!({"choices": [{"delta": {"tool_calls": [
                {"index": 0, "function": {"arguments": "me\":\"Keanu\"}"}}
            ]}, "finish_reason": "tool_calls"}]}),
            ));
        } else {
            events.push(("", json!({"choices": [{"delta": {}, "finish_reason": "stop"}]})));
        }
        events.push((
            "",
            json!({"choices": [], "usage": {"prompt_tokens": 11, "completion_tokens": 4}}),
        ));
        events.push(("", json!("[DONE]")));
        return sse(&events);
    }
    if tools {
        return Json(json!({"model": "fake-1", "choices": [{"finish_reason": "tool_calls", "message": {
            "content": null,
            "tool_calls": [{"id": "call_t", "type": "function", "function": {"name": "get_time", "arguments": "{}"}}]
        }}]})).into_response();
    }
    Json(json!({"model": "fake-1", "choices": [{"finish_reason": "stop", "message": {"content": "OK"}}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 1}}))
    .into_response()
}

async fn openai_models() -> Json<Value> {
    Json(json!({"data": [{"id": "b-model"}, {"id": "a-model"}]}))
}

async fn anthropic_messages(
    State(seen): State<Seen>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    seen.bodies.lock().unwrap().push(body.clone());
    seen.headers.lock().unwrap().push(headers.clone());
    if headers.get("x-api-key").and_then(|v| v.to_str().ok()) != Some("good")
        || headers.get("anthropic-version").is_none()
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"type": "error", "error": {"message": "invalid x-api-key"}})),
        )
            .into_response();
    }
    if body["model"] == "busy" {
        return (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "12")], "{}").into_response();
    }
    let tools = has_tools(&body);
    if body["stream"] == true {
        let mut events = vec![
            (
                "message_start",
                json!({"type": "message_start", "message": {"model": "claude-test", "usage": {"input_tokens": 20}}}),
            ),
            (
                "content_block_start",
                json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
            ),
            (
                "content_block_delta",
                json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "hmm"}}),
            ),
            (
                "content_block_delta",
                json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "sig-1"}}),
            ),
            ("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
            ("ping", json!({"type": "ping"})),
            (
                "content_block_start",
                json!({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Hel"}}),
            ),
            (
                "content_block_delta",
                json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "lo"}}),
            ),
            ("content_block_stop", json!({"type": "content_block_stop", "index": 1})),
        ];
        let stop = if tools {
            events.push(("content_block_start", json!({"type": "content_block_start", "index": 2, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "find", "input": {}}})));
            events.push(("content_block_delta", json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "{\"name\": \"Ke"}})));
            events.push(("content_block_delta", json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "anu\"}"}})));
            events.push(("content_block_stop", json!({"type": "content_block_stop", "index": 2})));
            "tool_use"
        } else {
            "end_turn"
        };
        events.push(("message_delta", json!({"type": "message_delta", "delta": {"stop_reason": stop}, "usage": {"output_tokens": 7}})));
        events.push(("message_stop", json!({"type": "message_stop"})));
        return sse(&events);
    }
    if tools {
        return Json(json!({"model": "claude-test", "stop_reason": "tool_use", "content": [
            {"type": "tool_use", "id": "toolu_t", "name": "get_time", "input": {}}
        ]}))
        .into_response();
    }
    Json(json!({"model": "claude-test", "stop_reason": "end_turn", "content": [{"type": "text", "text": "OK"}],
        "usage": {"input_tokens": 3, "output_tokens": 1}}))
    .into_response()
}

async fn anthropic_models() -> Json<Value> {
    Json(json!({"data": [{"id": "claude-test"}]}))
}

async fn fake() -> (String, Seen) {
    let seen = Seen::default();
    let app = Router::new()
        .route("/v1/chat/completions", post(openai_chat))
        .route("/v1/models", get(models))
        .route("/v1/messages", post(anthropic_messages))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), seen)
}

/// Both protocols use `/v1/models`; the key header tells them apart.
async fn models(headers: HeaderMap) -> Json<Value> {
    if headers.contains_key("x-api-key") { anthropic_models().await } else { openai_models().await }
}

fn provider(kind: ProviderKind, base: &str, key: &str, model: &str) -> Provider {
    Provider::new(ProviderConfig {
        kind,
        base_url: base.into(),
        api_key: Some(key.into()),
        model: model.into(),
        effort: None,
    })
    .unwrap()
}

fn find_tool() -> ToolSpec {
    ToolSpec {
        name: "find".into(),
        description: "Find titles with a person".into(),
        parameters: json!({"type": "object", "properties": {"name": {"type": "string"}}}),
    }
}

#[tokio::test]
async fn openai_compatible_streams_text_and_tool_calls() {
    let (base, seen) = fake().await;
    let p = provider(ProviderKind::OpenaiCompatible, &format!("{base}/v1/"), "good", "fake");
    let request = ChatRequest {
        messages: vec![Message::user("Who?")],
        tools: vec![find_tool()],
        max_tokens: 50,
    };
    let mut pieces = Vec::new();
    let mut sink = |s: &str| pieces.push(s.to_owned());
    let r = p.chat(&request, Some(&mut sink)).await.unwrap();
    assert_eq!(pieces, ["Hel", "lo"]);
    assert_eq!(r.text, "Hello");
    assert_eq!(r.stop, StopReason::ToolUse);
    assert_eq!(r.tool_calls[0].id, "call_a");
    assert_eq!(r.tool_calls[0].arguments, json!({"name": "Keanu"}));
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (11, 4));
    assert_eq!(r.model.as_deref(), Some("fake-1"));
    assert_eq!(seen.bodies.lock().unwrap()[0]["stream_options"]["include_usage"], true);

    // The tool result goes back on the next turn.
    let mut messages = request.messages.clone();
    messages.push(r.to_message());
    messages.push(Message::tool_result("call_a", "The Matrix"));
    let plain =
        p.chat(&ChatRequest { messages, tools: Vec::new(), max_tokens: 50 }, None).await.unwrap();
    assert_eq!(plain.text, "OK");
    let last = seen.bodies.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        last["messages"][2],
        json!({"role": "tool", "tool_call_id": "call_a", "content": "The Matrix"})
    );

    assert_eq!(p.list_models().await.unwrap(), ["a-model", "b-model"]);
    let report = p.test().await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.reply.as_deref(), Some("OK"));
    assert_eq!(report.tools_supported, Some(true));
}

#[tokio::test]
async fn anthropic_streams_and_replays_thinking_blocks() {
    let (base, seen) = fake().await;
    let p = provider(ProviderKind::Anthropic, &base, "good", "claude-opus-5-5");
    let request = ChatRequest {
        messages: vec![Message::system("Be brief."), Message::user("Who?")],
        tools: vec![find_tool()],
        max_tokens: 50,
    };
    let mut pieces = String::new();
    let mut sink = |s: &str| pieces.push_str(s);
    let r = p.chat(&request, Some(&mut sink)).await.unwrap();
    assert_eq!(pieces, "Hello");
    assert_eq!(r.stop, StopReason::ToolUse);
    assert_eq!(r.tool_calls[0].arguments, json!({"name": "Keanu"}));
    assert_eq!((r.usage.input_tokens, r.usage.output_tokens), (20, 7));
    {
        let headers = seen.headers.lock().unwrap();
        assert_eq!(headers[0]["anthropic-version"], "2026-10-21");
        assert_eq!(headers[0]["anthropic-beta"], "server-side-fallback-2026-07-01");
        let body = &seen.bodies.lock().unwrap()[0];
        assert_eq!(body["system"], "Be brief.");
        assert_eq!(body["fallbacks"], "default");
    }

    let mut messages = request.messages.clone();
    messages.push(r.to_message());
    messages.push(Message::tool_result("toolu_1", "The Matrix"));
    p.chat(&ChatRequest { messages, tools: vec![find_tool()], max_tokens: 50 }, None)
        .await
        .unwrap();
    let last = seen.bodies.lock().unwrap().last().unwrap().clone();
    let assistant = &last["messages"][1]["content"];
    assert_eq!(assistant[0], json!({"type": "thinking", "thinking": "hmm", "signature": "sig-1"}));
    assert_eq!(assistant[2]["input"], json!({"name": "Keanu"}));
    assert_eq!(last["messages"][2]["content"][0]["tool_use_id"], "toolu_1");

    assert_eq!(p.list_models().await.unwrap(), ["claude-test"]);
    let report = p.test().await;
    assert!(report.ok && report.tools_supported == Some(true), "{report:?}");
}

#[tokio::test]
async fn errors_are_explained() {
    let (base, _) = fake().await;
    let request =
        ChatRequest { messages: vec![Message::user("hi")], tools: Vec::new(), max_tokens: 10 };

    let wrong_key = provider(ProviderKind::OpenaiCompatible, &format!("{base}/v1"), "bad", "fake");
    let report = wrong_key.test().await;
    assert!(!report.ok);
    assert!(report.error.unwrap().contains("rejected"));

    let missing =
        provider(ProviderKind::OpenaiCompatible, &format!("{base}/v1"), "good", "missing");
    let e = missing.chat(&request, None).await.unwrap_err().to_string();
    assert!(e.contains("model 'missing' not found"), "{e}");

    let busy = provider(ProviderKind::Anthropic, &base, "good", "busy");
    let e = busy.chat(&request, None).await.unwrap_err();
    assert!(matches!(e, mbl_core::Error::RateLimited { retry_after_secs: 12 }), "{e}");

    let nobody = provider(ProviderKind::OpenaiCompatible, "http://127.0.0.1:9/v1", "good", "fake");
    let report = nobody.test().await;
    assert!(report.error.unwrap().contains("Could not reach"));

    assert!(
        Provider::new(ProviderConfig {
            kind: ProviderKind::OpenaiCompatible,
            base_url: base,
            api_key: None,
            model: " ".into(),
            effort: None,
        })
        .is_err()
    );
}
