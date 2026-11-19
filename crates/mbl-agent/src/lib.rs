//! Language model providers for MBL's built-in assistant.
//!
//! Two wire protocols cover nearly every provider:
//! - **OpenAI-compatible** chat completions: OpenAI, Google Gemini (OpenAI endpoint),
//!   OpenRouter, Groq, Mistral, DeepSeek and local servers such as Ollama, LM Studio, vLLM and
//!   llama.cpp.
//! - **Anthropic Messages**: Claude.
//!
//! [`Provider::chat`] sends one request, optionally streaming text as it is written, and
//! returns the text and any tool calls. The agent loop (tool execution) lives elsewhere.

mod anthropic;
mod openai;
mod sse;

use std::time::{Duration, Instant};

use mbl_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Wire protocol of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// `POST {base}/chat/completions` with `tools`.
    OpenaiCompatible,
    /// `POST {base}/v1/messages`.
    Anthropic,
}

/// Where and how to reach a model.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    /// For OpenAI-compatible servers including the version path, e.g.
    /// `https://api.openai.com/v1` or `http://localhost:11434/v1`. For Anthropic the host, e.g.
    /// `https://api.anthropic.com`.
    pub base_url: String,
    /// Not needed by most local servers.
    pub api_key: Option<String>,
    pub model: String,
    /// Reasoning effort (`low`, `medium`, `high`…), sent only when set.
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// The result of a tool call; `tool_call_id` names the call.
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Tool calls made in an assistant turn.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    /// The provider's own content blocks of an assistant turn. Anthropic requires thinking
    /// blocks to be sent back unchanged, so they are replayed from here when present.
    #[serde(default)]
    pub raw: Option<Value>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self::plain(Role::System, text)
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::plain(Role::User, text)
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::plain(Role::Assistant, text)
    }

    /// The result of the tool call `call_id`.
    pub fn tool_result(call_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self { tool_call_id: Some(call_id.into()), ..Self::plain(Role::Tool, text) }
    }

    fn plain(role: Role, text: impl Into<String>) -> Self {
        Self { role, content: text.into(), tool_calls: Vec::new(), tool_call_id: None, raw: None }
    }
}

/// A function the model may call. `parameters` is a JSON Schema object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Parsed arguments; a JSON string when the model wrote invalid JSON.
    pub arguments: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    /// The provider declined to answer.
    Refusal,
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop: StopReason,
    pub usage: Usage,
    /// Model that answered, as reported by the provider.
    pub model: Option<String>,
    raw: Option<Value>,
}

impl ChatResponse {
    /// The assistant turn to append to the conversation before sending tool results.
    pub fn to_message(&self) -> Message {
        Message {
            role: Role::Assistant,
            content: self.text.clone(),
            tool_calls: self.tool_calls.clone(),
            tool_call_id: None,
            raw: self.raw.clone(),
        }
    }
}

/// Outcome of [`Provider::test`].
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionReport {
    pub ok: bool,
    pub model: String,
    /// Time for the first answer.
    pub latency_ms: u64,
    /// What the model answered to the test question.
    pub reply: Option<String>,
    /// Whether the model called a tool when asked to; `None` when the first request failed.
    pub tools_supported: Option<bool>,
    pub error: Option<String>,
}

/// Called with each piece of text as it is streamed.
pub type TextSink<'a> = &'a mut (dyn FnMut(&str) + Send);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct Provider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl Provider {
    pub fn new(mut config: ProviderConfig) -> Result<Self> {
        config.base_url = normalize_base_url(config.kind, &config.base_url)?;
        config.model = config.model.trim().to_owned();
        if config.model.is_empty() {
            return Err(Error::Validation("Choose a model".into()));
        }
        config.api_key = config.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
        config.effort = config.effort.map(|e| e.trim().to_owned()).filter(|e| !e.is_empty());
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(Error::internal)?;
        Ok(Self { config, http })
    }

    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }

    /// Sends one request. With `on_text` the answer is streamed and each piece of text is
    /// passed on as it arrives; the full response is returned either way.
    pub async fn chat(
        &self,
        request: &ChatRequest,
        on_text: Option<TextSink<'_>>,
    ) -> Result<ChatResponse> {
        match self.config.kind {
            ProviderKind::OpenaiCompatible => {
                openai::chat(&self.http, &self.config, request, on_text).await
            }
            ProviderKind::Anthropic => {
                anthropic::chat(&self.http, &self.config, request, on_text).await
            }
        }
    }

    /// Model names the provider offers.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let (url, headers) = match self.config.kind {
            ProviderKind::OpenaiCompatible => {
                (format!("{}/models", self.config.base_url), openai::headers(&self.config))
            }
            ProviderKind::Anthropic => (
                format!("{}/v1/models?limit=100", self.config.base_url),
                anthropic::headers(&self.config),
            ),
        };
        let response = self.http.get(&url).headers(headers).send().await.map_err(send_error)?;
        let body = check(response).await?;
        let mut models: Vec<String> = body["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str().map(str::to_owned))
            .collect();
        models.sort();
        models.dedup();
        Ok(models)
    }

    /// Asks a short question, then whether the model calls a tool when told to.
    pub async fn test(&self) -> ConnectionReport {
        let model = self.config.model.clone();
        let started = Instant::now();
        let ask = ChatRequest {
            messages: vec![Message::user("Reply with the single word OK.")],
            tools: Vec::new(),
            max_tokens: 4000,
        };
        let reply = match self.chat(&ask, None).await {
            Ok(response) => response,
            Err(e) => {
                return ConnectionReport {
                    ok: false,
                    model,
                    latency_ms: elapsed_ms(started),
                    reply: None,
                    tools_supported: None,
                    error: Some(error_text(&e)),
                };
            }
        };
        let latency_ms = elapsed_ms(started);
        let tool = ChatRequest {
            messages: vec![Message::user(
                "Call the get_time tool to find out what time it is. Do not answer without it.",
            )],
            tools: vec![ToolSpec {
                name: "get_time".into(),
                description: "Returns the current time.".into(),
                parameters: serde_json::json!({ "type": "object", "properties": {} }),
            }],
            max_tokens: 4000,
        };
        let tools_supported = match self.chat(&tool, None).await {
            Ok(response) => response.tool_calls.iter().any(|c| c.name == "get_time"),
            Err(e) => {
                tracing::debug!(error = %e, "assistant tool test failed");
                false
            }
        };
        ConnectionReport {
            ok: true,
            model: reply.model.unwrap_or(model),
            latency_ms,
            reply: Some(reply.text.trim().chars().take(200).collect()),
            tools_supported: Some(tools_supported),
            error: None,
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn error_text(e: &Error) -> String {
    match e {
        Error::External(msg) | Error::Validation(msg) => msg.clone(),
        Error::RateLimited { retry_after_secs } => {
            format!("The provider is rate limiting; try again in {retry_after_secs} s")
        }
        other => other.to_string(),
    }
}

/// Trims the address and checks that it is http(s). For Anthropic a trailing `/v1` is dropped
/// since the paths add it.
pub fn normalize_base_url(kind: ProviderKind, raw: &str) -> Result<String> {
    let mut url = raw.trim().trim_end_matches('/').to_owned();
    if url.is_empty() {
        return Err(Error::Validation("Enter the provider's address".into()));
    }
    let parsed = reqwest::Url::parse(&url)
        .map_err(|_| Error::Validation(format!("'{url}' is not a valid address")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(Error::Validation("The address must start with http:// or https://".into()));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(Error::Validation("Put the key in the API key field, not the address".into()));
    }
    if kind == ProviderKind::Anthropic && url.ends_with("/v1") {
        url.truncate(url.len() - 3);
    }
    Ok(url)
}

fn send_error(e: reqwest::Error) -> Error {
    if e.is_timeout() {
        Error::External("The provider did not answer in time".into())
    } else {
        Error::External(format!("Could not reach the provider: {}", e.without_url()))
    }
}

/// Turns an error status into a readable error, otherwise returns the JSON body.
async fn check(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    if status.is_success() {
        return response
            .json()
            .await
            .map_err(|_| Error::External("The provider did not answer with JSON".into()));
    }
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let body = response.text().await.unwrap_or_default();
    Err(status_error(status, &body, retry_after))
}

/// The error for an unsuccessful response.
async fn failed(response: reqwest::Response) -> Error {
    match check(response).await {
        Err(e) => e,
        Ok(_) => Error::External("The provider answered with an unexpected status".into()),
    }
}

fn status_error(status: reqwest::StatusCode, body: &str, retry_after: Option<u64>) -> Error {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .or_else(|| v["message"].as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(200).collect());
    let detail = detail.trim();
    match status.as_u16() {
        401 | 403 => Error::External(format!("The API key was rejected ({status})")),
        404 => Error::External(format!("Not found: check the address and model ({detail})")),
        429 => Error::RateLimited { retry_after_secs: retry_after.unwrap_or(30) },
        _ if detail.is_empty() => Error::External(format!("The provider answered {status}")),
        _ => Error::External(format!("The provider answered {status}: {detail}")),
    }
}

/// Arguments as JSON; the raw text when the model wrote something that does not parse.
fn parse_arguments(raw: &str) -> Value {
    let raw = raw.trim();
    if raw.is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls_are_checked_and_trimmed() {
        let openai = ProviderKind::OpenaiCompatible;
        assert_eq!(
            normalize_base_url(openai, " http://localhost:11434/v1/ ").unwrap(),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            normalize_base_url(ProviderKind::Anthropic, "https://api.anthropic.com/v1").unwrap(),
            "https://api.anthropic.com"
        );
        assert!(normalize_base_url(openai, "").is_err());
        assert!(normalize_base_url(openai, "ftp://example.com").is_err());
        assert!(normalize_base_url(openai, "https://user:key@example.com/v1").is_err());
    }

    #[test]
    fn provider_errors_are_readable() {
        let body = r#"{"error":{"message":"model 'x' not found"}}"#;
        let e = status_error(reqwest::StatusCode::NOT_FOUND, body, None);
        assert!(error_text(&e).contains("model 'x' not found"), "{e}");
        let e = status_error(reqwest::StatusCode::UNAUTHORIZED, body, None);
        assert!(error_text(&e).contains("rejected"));
        let e = status_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "", Some(7));
        assert!(matches!(e, Error::RateLimited { retry_after_secs: 7 }));
    }

    #[test]
    fn invalid_tool_arguments_are_kept_as_text() {
        assert_eq!(parse_arguments(""), serde_json::json!({}));
        assert_eq!(parse_arguments(r#"{"a":1}"#), serde_json::json!({"a": 1}));
        assert_eq!(parse_arguments("{oops"), Value::String("{oops".into()));
    }
}
