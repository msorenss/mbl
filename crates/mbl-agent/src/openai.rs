//! OpenAI-compatible chat completions (`POST {base}/chat/completions`).

use std::collections::BTreeMap;

use mbl_core::{Error, Result};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use crate::{
    ChatRequest, ChatResponse, Message, ProviderConfig, Role, StopReason, TextSink, ToolCall,
    Usage, check, failed, parse_arguments, send_error, sse,
};

pub fn headers(config: &ProviderConfig) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(key) = &config.api_key
        && let Ok(value) = HeaderValue::from_str(&format!("Bearer {key}"))
    {
        headers.insert(AUTHORIZATION, value);
    }
    headers
}

fn message(m: &Message) -> Value {
    match m.role {
        Role::System => json!({ "role": "system", "content": m.content }),
        Role::User => json!({ "role": "user", "content": m.content }),
        Role::Tool => json!({
            "role": "tool",
            "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
            "content": m.content,
        }),
        Role::Assistant => {
            let mut out = json!({
                "role": "assistant",
                "content": if m.content.is_empty() { Value::Null } else { json!(m.content) },
            });
            if !m.tool_calls.is_empty() {
                out["tool_calls"] = m
                    .tool_calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                // The protocol carries arguments as a JSON string.
                                "arguments": match &c.arguments {
                                    Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                },
                            },
                        })
                    })
                    .collect();
            }
            out
        }
    }
}

pub fn body(config: &ProviderConfig, request: &ChatRequest, stream: bool) -> Value {
    let mut body = json!({
        "model": config.model,
        "messages": request.messages.iter().map(message).collect::<Vec<_>>(),
    });
    // OpenAI's own API wants the newer field name; other servers know `max_tokens`.
    let limit_field = if config.base_url.contains("api.openai.com") {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    body[limit_field] = json!(request.max_tokens);
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": { "name": t.name, "description": t.description, "parameters": t.parameters },
                })
            })
            .collect();
    }
    if let Some(effort) = &config.effort {
        body["reasoning_effort"] = json!(effort);
    }
    if stream {
        body["stream"] = json!(true);
        body["stream_options"] = json!({ "include_usage": true });
    }
    body
}

fn stop_reason(raw: Option<&str>) -> StopReason {
    match raw {
        Some("stop") | None => StopReason::EndTurn,
        Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_owned()),
    }
}

fn usage(v: &Value) -> Usage {
    let n = |key: &str| v[key].as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
    Usage { input_tokens: n("prompt_tokens"), output_tokens: n("completion_tokens") }
}

pub fn parse_response(v: &Value) -> Result<ChatResponse> {
    let choice = v["choices"]
        .get(0)
        .ok_or_else(|| Error::External("The provider returned no answer".into()))?;
    let msg = &choice["message"];
    let tool_calls = msg["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, c)| ToolCall {
            id: c["id"].as_str().map(str::to_owned).unwrap_or_else(|| format!("call_{i}")),
            name: c["function"]["name"].as_str().unwrap_or_default().to_owned(),
            arguments: parse_arguments(c["function"]["arguments"].as_str().unwrap_or_default()),
        })
        .collect::<Vec<_>>();
    let mut stop = stop_reason(choice["finish_reason"].as_str());
    // Some servers report "stop" even when they called a tool.
    if !tool_calls.is_empty() && stop == StopReason::EndTurn {
        stop = StopReason::ToolUse;
    }
    Ok(ChatResponse {
        text: msg["content"].as_str().unwrap_or_default().to_owned(),
        tool_calls,
        stop,
        usage: usage(&v["usage"]),
        model: v["model"].as_str().map(str::to_owned),
        raw: None,
    })
}

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

pub async fn chat(
    http: &reqwest::Client,
    config: &ProviderConfig,
    request: &ChatRequest,
    on_text: Option<TextSink<'_>>,
) -> Result<ChatResponse> {
    let url = format!("{}/chat/completions", config.base_url);
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

    let mut text = String::new();
    let mut calls: BTreeMap<u64, PartialCall> = BTreeMap::new();
    let mut finish: Option<String> = None;
    let mut totals = Usage::default();
    let mut model = None;
    sse::read(response, |event| {
        if event.data.trim() == "[DONE]" {
            return Ok(false);
        }
        let chunk: Value = serde_json::from_str(&event.data)
            .map_err(|_| Error::External("The provider sent an unreadable event".into()))?;
        if let Some(message) = chunk["error"]["message"].as_str() {
            return Err(Error::External(format!("The provider reported: {message}")));
        }
        if model.is_none() {
            model = chunk["model"].as_str().map(str::to_owned);
        }
        if chunk["usage"].is_object() {
            totals = usage(&chunk["usage"]);
        }
        let Some(choice) = chunk["choices"].get(0) else { return Ok(true) };
        let delta = &choice["delta"];
        if let Some(piece) = delta["content"].as_str()
            && !piece.is_empty()
        {
            text.push_str(piece);
            on_text(piece);
        }
        for c in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = c["index"].as_u64().unwrap_or(0);
            let call = calls.entry(index).or_default();
            if let Some(id) = c["id"].as_str() {
                call.id = id.to_owned();
            }
            if let Some(name) = c["function"]["name"].as_str() {
                call.name.push_str(name);
            }
            if let Some(args) = c["function"]["arguments"].as_str() {
                call.arguments.push_str(args);
            }
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            finish = Some(reason.to_owned());
        }
        Ok(true)
    })
    .await?;

    let tool_calls: Vec<ToolCall> = calls
        .into_iter()
        .map(|(i, c)| ToolCall {
            id: if c.id.is_empty() { format!("call_{i}") } else { c.id },
            name: c.name,
            arguments: parse_arguments(&c.arguments),
        })
        .collect();
    let mut stop = stop_reason(finish.as_deref());
    if !tool_calls.is_empty() && stop == StopReason::EndTurn {
        stop = StopReason::ToolUse;
    }
    Ok(ChatResponse { text, tool_calls, stop, usage: totals, model, raw: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProviderKind, ToolSpec};

    fn config(base: &str) -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::OpenaiCompatible,
            base_url: base.into(),
            api_key: Some("k".into()),
            model: "m".into(),
            effort: None,
        }
    }

    #[test]
    fn requests_carry_tools_and_tool_results() {
        let request = ChatRequest {
            messages: vec![
                Message::system("be brief"),
                Message::user("hi"),
                Message {
                    tool_calls: vec![ToolCall {
                        id: "c1".into(),
                        name: "f".into(),
                        arguments: json!({"x": 1}),
                    }],
                    ..Message::assistant("")
                },
                Message::tool_result("c1", "42"),
            ],
            tools: vec![ToolSpec {
                name: "f".into(),
                description: "d".into(),
                parameters: json!({"type": "object"}),
            }],
            max_tokens: 100,
        };
        let b = body(&config("http://localhost:11434/v1"), &request, false);
        assert_eq!(b["max_tokens"], 100);
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["messages"][2]["content"], Value::Null);
        assert_eq!(b["messages"][2]["tool_calls"][0]["function"]["arguments"], r#"{"x":1}"#);
        assert_eq!(
            b["messages"][3],
            json!({"role": "tool", "tool_call_id": "c1", "content": "42"})
        );
        assert_eq!(b["tools"][0]["function"]["name"], "f");
        let b = body(&config("https://api.openai.com/v1"), &request, true);
        assert_eq!(b["max_completion_tokens"], 100);
        assert_eq!(b["stream"], true);
    }

    #[test]
    fn responses_with_tool_calls_are_parsed() {
        let r = parse_response(&json!({
            "model": "m",
            "choices": [{"finish_reason": "stop", "message": {"content": null, "tool_calls": [
                {"id": "a", "function": {"name": "f", "arguments": "{\"q\":\"x\"}"}}
            ]}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2}
        }))
        .unwrap();
        assert_eq!(r.stop, StopReason::ToolUse);
        assert_eq!(r.tool_calls[0].arguments, json!({"q": "x"}));
        assert_eq!(r.usage, Usage { input_tokens: 5, output_tokens: 2 });
    }
}
