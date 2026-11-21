//! Anthropic Messages API (`POST {base}/v1/messages`).

use std::collections::BTreeMap;

use mbl_core::{Error, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value, json};

use crate::{
    ChatRequest, ChatResponse, Message, ProviderConfig, Role, StopReason, TextSink, ToolCall,
    Usage, check, failed, is_json, parse_arguments, send_error, sse,
};

const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Models that accept `fallbacks: "default"`, which reruns a declined request on a fallback
/// model instead of stopping.
const FALLBACK_MODELS: &[&str] = &[
    "claude-fable-5-2",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];

fn uses_fallbacks(model: &str) -> bool {
    FALLBACK_MODELS.contains(&model)
}

pub fn headers(config: &ProviderConfig) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(API_VERSION),
    );
    if let Some(key) = &config.api_key
        && let Ok(value) = HeaderValue::from_str(key)
    {
        headers.insert(HeaderName::from_static("x-api-key"), value);
    }
    if uses_fallbacks(&config.model) {
        headers.insert(
            HeaderName::from_static("anthropic-beta"),
            HeaderValue::from_static(FALLBACK_BETA),
        );
    }
    headers
}

/// Content blocks for one message.
fn blocks(m: &Message) -> Vec<Value> {
    match m.role {
        Role::Tool if !m.images.is_empty() => vec![json!({
            "type": "tool_result",
            "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
            "content": with_images(&m.content, &m.images),
        })],
        Role::Tool => vec![json!({
            "type": "tool_result",
            "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
            "content": m.content,
        })],
        Role::Assistant => {
            // Thinking blocks must go back exactly as received.
            if let Some(Value::Array(raw)) = &m.raw {
                return raw.clone();
            }
            let mut out = Vec::new();
            if !m.content.is_empty() {
                out.push(json!({ "type": "text", "text": m.content }));
            }
            for c in &m.tool_calls {
                let input = match &c.arguments {
                    Value::Object(_) => c.arguments.clone(),
                    _ => json!({}),
                };
                out.push(json!({ "type": "tool_use", "id": c.id, "name": c.name, "input": input }));
            }
            out
        }
        Role::User => with_images(&m.content, &m.images),
        Role::System => vec![json!({ "type": "text", "text": m.content })],
    }
}

/// Image blocks followed by the text (the order Anthropic recommends).
fn with_images(text: &str, images: &[crate::Image]) -> Vec<Value> {
    let mut out: Vec<Value> = images
        .iter()
        .map(|i| {
            json!({
                "type": "image",
                "source": { "type": "base64", "media_type": i.media_type, "data": i.data },
            })
        })
        .collect();
    out.push(json!({ "type": "text", "text": text }));
    out
}

pub fn body(config: &ProviderConfig, request: &ChatRequest, stream: bool) -> Value {
    let system: Vec<&str> = request
        .messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect();
    // Roles must alternate; tool results travel in a user turn, so consecutive turns of the
    // same side are merged (all results of one assistant turn end up in one message).
    let mut messages: Vec<(&'static str, Vec<Value>)> = Vec::new();
    for m in request.messages.iter().filter(|m| m.role != Role::System) {
        let role = if m.role == Role::Assistant { "assistant" } else { "user" };
        let content = blocks(m);
        match messages.last_mut() {
            Some((last, list)) if *last == role => list.extend(content),
            _ => messages.push((role, content)),
        }
    }
    let mut body = json!({
        "model": config.model,
        "max_tokens": request.max_tokens,
        "messages": messages
            .into_iter()
            .map(|(role, content)| json!({ "role": role, "content": content }))
            .collect::<Vec<_>>(),
    });
    if !system.is_empty() {
        body["system"] = json!(system.join("\n\n"));
    }
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.parameters }))
            .collect();
    }
    if let Some(effort) = &config.effort {
        body["output_config"] = json!({ "effort": effort });
    }
    if uses_fallbacks(&config.model) {
        body["fallbacks"] = json!("default");
    }
    if stream {
        body["stream"] = json!(true);
    }
    body
}

fn stop_reason(raw: Option<&str>) -> StopReason {
    match raw {
        Some("end_turn") | Some("stop_sequence") | None => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("refusal") => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_owned()),
    }
}

fn count(v: &Value) -> u32 {
    v.as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0)
}

/// Text and tool calls from finished content blocks.
fn summarize(content: &[Value]) -> (String, Vec<ToolCall>) {
    let mut text = String::new();
    let mut calls = Vec::new();
    for block in content {
        match block["type"].as_str() {
            Some("text") => text.push_str(block["text"].as_str().unwrap_or_default()),
            Some("tool_use") => calls.push(ToolCall {
                id: block["id"].as_str().unwrap_or_default().to_owned(),
                name: block["name"].as_str().unwrap_or_default().to_owned(),
                arguments: block["input"].clone(),
            }),
            _ => {}
        }
    }
    (text, calls)
}

pub fn parse_response(v: &Value) -> Result<ChatResponse> {
    let content = v["content"]
        .as_array()
        .cloned()
        .ok_or_else(|| Error::External("The provider returned no answer".into()))?;
    let (text, tool_calls) = summarize(&content);
    Ok(ChatResponse {
        text,
        tool_calls,
        stop: stop_reason(v["stop_reason"].as_str()),
        usage: Usage {
            input_tokens: count(&v["usage"]["input_tokens"]),
            output_tokens: count(&v["usage"]["output_tokens"]),
        },
        model: v["model"].as_str().map(str::to_owned),
        raw: Some(Value::Array(content)),
    })
}

fn append(block: &mut Value, field: &str, piece: &str) {
    if let Some(obj) = block.as_object_mut() {
        let current = obj.entry(field).or_insert_with(|| json!(""));
        let joined = format!("{}{piece}", current.as_str().unwrap_or_default());
        *current = json!(joined);
    }
}

pub async fn chat(
    http: &reqwest::Client,
    config: &ProviderConfig,
    request: &ChatRequest,
    on_text: Option<TextSink<'_>>,
) -> Result<ChatResponse> {
    let url = format!("{}/v1/messages", config.base_url);
    let stream = on_text.is_some();
    let response = http
        .post(&url)
        .headers(headers(config))
        .json(&body(config, request, stream))
        .send()
        .await
        .map_err(send_error)?;
    let Some(on_text) = on_text else {
        return parse_response(&check(response).await?);
    };
    if !response.status().is_success() {
        return Err(failed(response).await);
    }
    // Some servers ignore `stream` and answer with one JSON body.
    if is_json(&response) {
        let parsed = parse_response(&check(response).await?)?;
        if !parsed.text.is_empty() {
            on_text(&parsed.text);
        }
        return Ok(parsed);
    }

    // Blocks are rebuilt from their start event plus deltas, so that thinking blocks (with
    // their signatures) can be sent back unchanged in the next turn.
    let mut content: BTreeMap<u64, Value> = BTreeMap::new();
    let mut tool_json: BTreeMap<u64, String> = BTreeMap::new();
    let mut usage = Usage::default();
    let mut stop = None;
    let mut model = None;
    sse::read(response, |event| {
        let v: Value = serde_json::from_str(&event.data)
            .map_err(|_| Error::External("The provider sent an unreadable event".into()))?;
        let index = v["index"].as_u64().unwrap_or(0);
        match v["type"].as_str().unwrap_or_default() {
            "message_start" => {
                model = v["message"]["model"].as_str().map(str::to_owned);
                usage.input_tokens = count(&v["message"]["usage"]["input_tokens"]);
            }
            "content_block_start" => {
                let mut block = v["content_block"].clone();
                if block["type"] == "tool_use" {
                    // The input arrives as JSON text in deltas.
                    if let Some(obj) = block.as_object_mut() {
                        obj.insert("input".into(), Value::Object(Map::new()));
                    }
                    tool_json.insert(index, String::new());
                }
                content.insert(index, block);
            }
            "content_block_delta" => {
                let delta = &v["delta"];
                let Some(block) = content.get_mut(&index) else { return Ok(true) };
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        let piece = delta["text"].as_str().unwrap_or_default();
                        append(block, "text", piece);
                        if !piece.is_empty() {
                            on_text(piece);
                        }
                    }
                    "thinking_delta" => {
                        append(block, "thinking", delta["thinking"].as_str().unwrap_or_default());
                    }
                    "signature_delta" => {
                        block["signature"] = delta["signature"].clone();
                    }
                    "input_json_delta" => {
                        if let Some(buf) = tool_json.get_mut(&index) {
                            buf.push_str(delta["partial_json"].as_str().unwrap_or_default());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let (Some(buf), Some(block)) =
                    (tool_json.remove(&index), content.get_mut(&index))
                {
                    block["input"] = match parse_arguments(&buf) {
                        Value::String(_) => {
                            return Err(Error::External(
                                "The model wrote tool input that is not valid JSON".into(),
                            ));
                        }
                        parsed => parsed,
                    };
                }
            }
            "message_delta" => {
                if let Some(reason) = v["delta"]["stop_reason"].as_str() {
                    stop = Some(reason.to_owned());
                }
                if v["usage"]["output_tokens"].is_u64() {
                    usage.output_tokens = count(&v["usage"]["output_tokens"]);
                }
            }
            "message_stop" => return Ok(false),
            "error" => {
                let message = v["error"]["message"].as_str().unwrap_or("unknown error");
                return Err(Error::External(format!("The provider reported: {message}")));
            }
            _ => {}
        }
        Ok(true)
    })
    .await?;

    let content: Vec<Value> = content.into_values().collect();
    let (text, tool_calls) = summarize(&content);
    Ok(ChatResponse {
        text,
        tool_calls,
        stop: stop_reason(stop.as_deref()),
        usage,
        model,
        raw: Some(Value::Array(content)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProviderKind, ToolSpec};

    fn config(model: &str) -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            api_key: Some("k".into()),
            model: model.into(),
            effort: Some("low".into()),
        }
    }

    #[test]
    fn images_go_into_user_turns_and_tool_results() {
        let image = crate::Image { media_type: "image/jpeg".into(), data: "QUJD".into() };
        let call = crate::ToolCall { id: "t1".into(), name: "frame".into(), arguments: json!({}) };
        let request = ChatRequest {
            messages: vec![
                Message::user("look").with_images(vec![image.clone()]),
                Message { tool_calls: vec![call], ..Message::assistant("") },
                Message::tool_result("t1", "frame").with_images(vec![image]),
            ],
            max_tokens: 100,
            ..Default::default()
        };
        let b = body(&config("claude-opus-5-5"), &request, false);
        let m = &b["messages"];
        assert_eq!(m[0]["content"][0]["type"], "image");
        assert_eq!(m[0]["content"][0]["source"]["data"], "QUJD");
        assert_eq!(m[0]["content"][1], json!({"type": "text", "text": "look"}));
        let result = &m[2]["content"][0];
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][1]["text"], "frame");
    }

    #[test]
    fn tool_results_are_merged_into_one_user_turn() {
        let thinking = json!([
            {"type": "thinking", "thinking": "", "signature": "sig"},
            {"type": "tool_use", "id": "t1", "name": "f", "input": {}},
            {"type": "tool_use", "id": "t2", "name": "g", "input": {}}
        ]);
        let request = ChatRequest {
            messages: vec![
                Message::system("be brief"),
                Message::user("hi"),
                Message { raw: Some(thinking.clone()), ..Message::assistant("") },
                Message::tool_result("t1", "a"),
                Message::tool_result("t2", "b"),
            ],
            tools: vec![ToolSpec {
                name: "f".into(),
                description: "d".into(),
                parameters: json!({"type": "object"}),
            }],
            max_tokens: 100,
        };
        let b = body(&config("claude-opus-5-5"), &request, false);
        assert_eq!(b["system"], "be brief");
        let messages = b["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["content"], thinking, "thinking replayed unchanged");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"].as_array().unwrap().len(), 2);
        assert_eq!(b["tools"][0]["input_schema"], json!({"type": "object"}));
        assert_eq!(b["output_config"]["effort"], "low");
        assert_eq!(b["fallbacks"], "default");
        assert!(b.get("temperature").is_none());
        assert!(headers(&config("claude-opus-5-5")).contains_key("anthropic-beta"));
        let older = body(&config("claude-haiku-4-5"), &request, false);
        assert!(older.get("fallbacks").is_none());
        assert!(!headers(&config("claude-haiku-4-5")).contains_key("anthropic-beta"));
    }

    #[test]
    fn responses_keep_raw_blocks_and_refusals() {
        let r = parse_response(&json!({
            "model": "claude-opus-5-5",
            "stop_reason": "tool_use",
            "content": [
                {"type": "thinking", "thinking": "", "signature": "s"},
                {"type": "text", "text": "Let me look."},
                {"type": "tool_use", "id": "t", "name": "f", "input": {"q": 1}}
            ],
            "usage": {"input_tokens": 9, "output_tokens": 3}
        }))
        .unwrap();
        assert_eq!(r.text, "Let me look.");
        assert_eq!(r.tool_calls[0].arguments, json!({"q": 1}));
        assert_eq!(r.to_message().raw.unwrap().as_array().unwrap().len(), 3);
        let refused = parse_response(&json!({"stop_reason": "refusal", "content": []})).unwrap();
        assert_eq!(refused.stop, StopReason::Refusal);
    }
}
