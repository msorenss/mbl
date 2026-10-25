//! Streamable HTTP server transport (`/mcp`), client transports (HTTP, router, stdio),
//! and stdio ↔ Streamable HTTP proxy.

use std::fmt::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, State};
use axum::http::{Extensions, HeaderMap, HeaderValue, Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use camino::Utf8PathBuf;
use http_body_util::BodyExt;
use mbl_auth::tokens::ScopeSet;
use mbl_core::{Actor, Error, LibraryAccess, Result, UserId};
use mbl_db::Db;
use mbl_jobs::JobQueue;
use mbl_service::Services;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tower::ServiceExt;

use crate::caller::{Caller, McpRateLimiter, SESSION_COOKIE, resolve_caller};
use crate::server::{
    CallToolResult, InitializeResult, JsonRpcError, JsonRpcRequest, JsonRpcResponse,
    ListToolsResult, MblMcp, PROTOCOL_VERSION, ResourceContents, ResourceDefinition,
    ResourceTemplateDefinition,
};

pub const MCP_SESSION_HEADER: &str = "mcp-session-id";

pub use mbl_service::McpConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkClass {
    Lan,
    Tailnet,
    Wan,
}

#[derive(Clone)]
pub struct McpState {
    pub db: Db,
    pub services: Services,
    config: Arc<RwLock<McpConfig>>,
    pub limiter: Arc<McpRateLimiter>,
    pub logs_dir: Option<Utf8PathBuf>,
    pub jobs: Option<JobQueue>,
}

impl McpState {
    pub fn new(db: Db, services: Services, config: McpConfig) -> Self {
        let handle = services.mcp.config_handle();
        if let Ok(mut g) = handle.write() {
            *g = config;
        }
        Self {
            db,
            services,
            config: handle,
            limiter: Arc::new(McpRateLimiter::default()),
            logs_dir: None,
            jobs: None,
        }
    }

    pub fn with_limiter(mut self, limiter: Arc<McpRateLimiter>) -> Self {
        self.limiter = limiter;
        self
    }

    pub fn with_logs_dir(mut self, logs_dir: Option<Utf8PathBuf>) -> Self {
        self.logs_dir = logs_dir;
        self
    }

    pub fn with_jobs(mut self, jobs: Option<JobQueue>) -> Self {
        self.jobs = jobs;
        self
    }

    pub fn config(&self) -> McpConfig {
        self.config.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn is_enabled(&self) -> bool {
        self.config.read().map(|g| g.enabled).unwrap_or(true)
    }

    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut guard) = self.config.write() {
            guard.enabled = enabled;
        }
    }
}

/// Build the Axum router mounting Streamable HTTP at `/mcp`.
pub fn router(state: McpState) -> Router {
    Router::new()
        .route("/mcp", post(handle_post).get(handle_get).delete(handle_delete))
        .with_state(state)
}

/// Classify an IP address into `Lan`, `Tailnet` (CGNAT `100.64.0.0/10` or `fd7a:115c:a1e0::/48`), or `Wan`.
pub fn classify_ip(ip: IpAddr) -> NetworkClass {
    match ip {
        IpAddr::V4(v4) => classify_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return classify_ipv4(mapped);
            }
            let segs = v6.segments();
            if segs[0] == 0xfd7a && segs[1] == 0x115c && segs[2] == 0xa1e0 {
                return NetworkClass::Tailnet;
            }
            if v6.is_loopback()
                || v6.is_unspecified()
                || (segs[0] & 0xfe00) == 0xfc00
                || (segs[0] & 0xffc0) == 0xfe80
            {
                NetworkClass::Lan
            } else {
                NetworkClass::Wan
            }
        }
    }
}

fn classify_ipv4(v4: Ipv4Addr) -> NetworkClass {
    let octets = v4.octets();
    if octets[0] == 100 && (octets[1] & 0b1100_0000) == 64 {
        return NetworkClass::Tailnet;
    }
    if v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() {
        NetworkClass::Lan
    } else {
        NetworkClass::Wan
    }
}

/// Extract client IP from `ConnectInfo<SocketAddr>` (or `X-Forwarded-For` / `X-Real-IP` when arriving on loopback/in-process).
pub fn extract_client_ip(headers: &HeaderMap, extensions: &Extensions) -> IpAddr {
    if let Some(ConnectInfo(addr)) = extensions.get::<ConnectInfo<SocketAddr>>()
        && !addr.ip().is_loopback()
    {
        return addr.ip();
    }
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = xff.split(',').next()
        && let Ok(ip) = first.trim().parse::<IpAddr>()
    {
        return ip;
    }
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok())
        && let Ok(ip) = real_ip.trim().parse::<IpAddr>()
    {
        return ip;
    }
    if let Some(ConnectInfo(addr)) = extensions.get::<ConnectInfo<SocketAddr>>() {
        return addr.ip();
    }
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

/// Validate `Origin` header per M2.11:
/// - Allow same-origin (matching `Host`).
/// - Allow configured `allowed_origins`.
/// - Allow requests without an `Origin` header that carry a Bearer token.
/// - Reject everything else.
pub fn validate_origin(headers: &HeaderMap, allowed_origins: &[String]) -> bool {
    match headers.get(header::ORIGIN) {
        Some(raw_origin) => {
            let Ok(origin) = raw_origin.to_str() else {
                return false;
            };
            let origin = origin.trim();
            if origin.is_empty() || origin.eq_ignore_ascii_case("null") {
                return false;
            }
            let origin_host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
            let origin_host = origin_host.trim_end_matches('/');

            if let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok())
                && host.trim().eq_ignore_ascii_case(origin_host)
            {
                return true;
            }

            allowed_origins.iter().any(|allowed| {
                let allowed = allowed.trim().trim_end_matches('/');
                if allowed.eq_ignore_ascii_case(origin.trim_end_matches('/')) {
                    return true;
                }
                let allowed_host = allowed.split_once("://").map_or(allowed, |(_, r)| r);
                allowed_host.eq_ignore_ascii_case(origin_host)
            })
        }
        None => has_bearer_token(headers) || has_session_cookie(headers),
    }
}

pub fn has_bearer_token(headers: &HeaderMap) -> bool {
    bearer_token(headers).is_some()
}

pub fn has_session_cookie(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(k, v)| k == SESSION_COOKIE && !v.is_empty())
}

pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty())
        .then(|| token.trim().to_owned())
}

fn check_guards(
    state: &McpState,
    headers: &HeaderMap,
    extensions: &Extensions,
) -> std::result::Result<NetworkClass, Box<Response>> {
    let cfg = state.config();
    if !cfg.enabled {
        return Err(Box::new(json_status(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": { "code": "mcp_disabled", "message": "MCP server is disabled" } }),
        )));
    }

    let client_ip = extract_client_ip(headers, extensions);
    let net_class = classify_ip(client_ip);
    if !cfg.allow_wan && net_class == NetworkClass::Wan {
        return Err(Box::new(json_status(
            StatusCode::FORBIDDEN,
            json!({ "error": { "code": "wan_forbidden", "message": "MCP is not allowed over WAN" } }),
        )));
    }

    if !validate_origin(headers, &cfg.allowed_origins) {
        return Err(Box::new(json_status(
            StatusCode::FORBIDDEN,
            json!({ "error": { "code": "invalid_origin", "message": "Origin not allowed or missing bearer token" } }),
        )));
    }

    Ok(net_class)
}

async fn resolve_or_anonymous_caller(
    state: &McpState,
    headers: &HeaderMap,
    net_class: NetworkClass,
) -> std::result::Result<Caller, Box<Response>> {
    if has_bearer_token(headers) || has_session_cookie(headers) {
        match resolve_caller(&state.services, headers, net_class).await {
            Ok(caller) => Ok(caller),
            Err(Error::Unauthorized) => Err(Box::new(json_status(
                StatusCode::UNAUTHORIZED,
                json!({ "error": { "code": "unauthorized", "message": "Invalid, expired, or revoked token" } }),
            ))),
            Err(Error::Forbidden) => Err(Box::new(json_status(
                StatusCode::FORBIDDEN,
                json!({ "error": { "code": "forbidden", "message": "Token not permitted on this network" } }),
            ))),
            Err(_) => Err(Box::new(json_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": { "code": "internal", "message": "Authentication error" } }),
            ))),
        }
    } else {
        Ok(Caller {
            actor: Actor {
                user_id: UserId(0),
                is_admin: false,
                library_access: LibraryAccess::Only(Vec::new()),
            },
            scopes: ScopeSet::default(),
            client: "anonymous".into(),
            token_id: None,
        })
    }
}

async fn handle_post(State(state): State<McpState>, req: Request<Body>) -> Response {
    let (parts, body) = req.into_parts();
    let net_class = match check_guards(&state, &parts.headers, &parts.extensions) {
        Ok(nc) => nc,
        Err(resp) => return *resp,
    };

    let caller = match resolve_or_anonymous_caller(&state, &parts.headers, net_class).await {
        Ok(c) => c,
        Err(resp) => return *resp,
    };

    let bytes: Bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return json_status(
                StatusCode::BAD_REQUEST,
                serde_json::to_value(JsonRpcResponse::err(
                    Value::Null,
                    JsonRpcError {
                        code: JsonRpcError::PARSE_ERROR,
                        message: "failed to read request body".into(),
                        data: None,
                    },
                ))
                .unwrap_or(Value::Null),
            );
        }
    };

    let parsed: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return json_status(
                StatusCode::BAD_REQUEST,
                serde_json::to_value(JsonRpcResponse::err(
                    Value::Null,
                    JsonRpcError {
                        code: JsonRpcError::PARSE_ERROR,
                        message: format!("invalid JSON: {e}"),
                        data: None,
                    },
                ))
                .unwrap_or(Value::Null),
            );
        }
    };

    let session_id = parts
        .headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(generate_session_id);

    let mcp = MblMcp::new(state.db.clone(), state.services.clone(), caller, state.limiter.clone())
        .with_logs_dir(state.logs_dir.clone())
        .with_jobs(state.jobs.clone());

    if let Some(arr) = parsed.as_array() {
        if arr.is_empty() {
            return json_status(
                StatusCode::BAD_REQUEST,
                serde_json::to_value(JsonRpcResponse::err(
                    Value::Null,
                    JsonRpcError {
                        code: JsonRpcError::INVALID_REQUEST,
                        message: "empty batch".into(),
                        data: None,
                    },
                ))
                .unwrap_or(Value::Null),
            );
        }
        let mut responses = Vec::new();
        for item in arr {
            match serde_json::from_value::<JsonRpcRequest>(item.clone()) {
                Ok(rpc_req) => {
                    if let Some(resp) = mcp.dispatch(rpc_req).await {
                        responses.push(resp);
                    }
                }
                Err(e) => {
                    responses.push(JsonRpcResponse::err(
                        Value::Null,
                        JsonRpcError {
                            code: JsonRpcError::INVALID_REQUEST,
                            message: format!("invalid request: {e}"),
                            data: None,
                        },
                    ));
                }
            }
        }
        if responses.is_empty() {
            return with_session_header(StatusCode::ACCEPTED.into_response(), &session_id);
        }
        let body = serde_json::to_value(responses).unwrap_or(Value::Null);
        return with_session_header(json_status(StatusCode::OK, body), &session_id);
    }

    let rpc_req: JsonRpcRequest = match serde_json::from_value(parsed) {
        Ok(r) => r,
        Err(e) => {
            return json_status(
                StatusCode::BAD_REQUEST,
                serde_json::to_value(JsonRpcResponse::err(
                    Value::Null,
                    JsonRpcError {
                        code: JsonRpcError::INVALID_REQUEST,
                        message: format!("invalid JSON-RPC request: {e}"),
                        data: None,
                    },
                ))
                .unwrap_or(Value::Null),
            );
        }
    };

    match mcp.dispatch(rpc_req).await {
        Some(resp) => {
            let body = serde_json::to_value(resp).unwrap_or(Value::Null);
            with_session_header(json_status(StatusCode::OK, body), &session_id)
        }
        None => with_session_header(StatusCode::ACCEPTED.into_response(), &session_id),
    }
}

async fn handle_get(State(state): State<McpState>, req: Request<Body>) -> Response {
    let (parts, _) = req.into_parts();
    if let Err(resp) = check_guards(&state, &parts.headers, &parts.extensions) {
        return *resp;
    }
    StatusCode::METHOD_NOT_ALLOWED.into_response()
}

async fn handle_delete(State(state): State<McpState>, req: Request<Body>) -> Response {
    let (parts, _) = req.into_parts();
    if let Err(resp) = check_guards(&state, &parts.headers, &parts.extensions) {
        return *resp;
    }
    StatusCode::NO_CONTENT.into_response()
}

fn json_status(status: StatusCode, body: Value) -> Response {
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn with_session_header(mut resp: Response, session_id: &str) -> Response {
    if let Ok(val) = HeaderValue::from_str(session_id) {
        resp.headers_mut().insert(MCP_SESSION_HEADER, val);
    }
    resp
}

fn generate_session_id() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    let mut hex = String::with_capacity(32);
    for b in bytes {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Normalize a server base URL or `/mcp` URL into the full `/mcp` endpoint URL.
pub fn normalize_mcp_url(server: &str) -> String {
    let trimmed = server.trim().trim_end_matches('/');
    if trimmed.ends_with("/mcp") { trimmed.to_owned() } else { format!("{trimmed}/mcp") }
}

enum ClientBackend {
    Http {
        client: reqwest::Client,
        url: String,
        token: Option<String>,
        session_id: Option<String>,
    },
    Router {
        router: Router,
        token: Option<String>,
        cookie: Option<String>,
        origin: Option<String>,
        host: Option<String>,
        connect_addr: Option<SocketAddr>,
        session_id: Option<String>,
    },
    Stdio {
        reader: Box<dyn AsyncBufRead + Unpin + Send>,
        writer: Box<dyn AsyncWrite + Unpin + Send>,
    },
}

/// MCP client supporting Streamable HTTP, in-process Axum router, and stdio transports.
pub struct McpClient {
    backend: ClientBackend,
    next_id: i64,
}

impl McpClient {
    pub fn connect_http(server_url: impl AsRef<str>, token: Option<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| Error::External(e.to_string()))?;
        Ok(Self {
            backend: ClientBackend::Http {
                client,
                url: normalize_mcp_url(server_url.as_ref()),
                token,
                session_id: None,
            },
            next_id: 1,
        })
    }

    pub fn connect_router(router: Router, token: Option<String>) -> Self {
        Self {
            backend: ClientBackend::Router {
                router,
                token,
                cookie: None,
                origin: None,
                host: None,
                connect_addr: None,
                session_id: None,
            },
            next_id: 1,
        }
    }

    pub fn connect_stdio<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        Self {
            backend: ClientBackend::Stdio { reader: Box::new(reader), writer: Box::new(writer) },
            next_id: 1,
        }
    }

    pub fn with_origin(mut self, host: impl Into<String>, origin: impl Into<String>) -> Self {
        if let ClientBackend::Router { host: h, origin: o, .. } = &mut self.backend {
            *h = Some(host.into());
            *o = Some(origin.into());
        }
        self
    }

    pub fn with_session_cookie(
        mut self,
        cookie_token: impl Into<String>,
        host: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        if let ClientBackend::Router { cookie, host: h, origin: o, .. } = &mut self.backend {
            *cookie = Some(cookie_token.into());
            *h = Some(host.into());
            *o = Some(origin.into());
        }
        self
    }

    pub fn with_connect_addr(mut self, addr: SocketAddr) -> Self {
        if let ClientBackend::Router { connect_addr, .. } = &mut self.backend {
            *connect_addr = Some(addr);
        }
        self
    }

    /// Perform the MCP `initialize` handshake followed by `notifications/initialized`.
    pub async fn initialize(&mut self) -> Result<InitializeResult> {
        let resp = self
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "mbl-mcp-client",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                })),
            )
            .await?;
        let init: InitializeResult = serde_json::from_value(resp)
            .map_err(|e| Error::Validation(format!("invalid InitializeResult: {e}")))?;
        self.notify("notifications/initialized", None).await?;
        Ok(init)
    }

    pub async fn list_tools(&mut self) -> Result<ListToolsResult> {
        let resp = self.request("tools/list", Some(json!({}))).await?;
        serde_json::from_value(resp)
            .map_err(|e| Error::Validation(format!("invalid ListToolsResult: {e}")))
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<CallToolResult> {
        let resp = self
            .request(
                "tools/call",
                Some(json!({
                    "name": name,
                    "arguments": arguments,
                })),
            )
            .await?;
        serde_json::from_value(resp)
            .map_err(|e| Error::Validation(format!("invalid CallToolResult: {e}")))
    }

    pub async fn list_resources(&mut self) -> Result<Vec<ResourceDefinition>> {
        let resp = self.request("resources/list", Some(json!({}))).await?;
        let res: Value = serde_json::from_value(resp)
            .map_err(|e| Error::Validation(format!("invalid resources/list response: {e}")))?;
        serde_json::from_value(res.get("resources").cloned().unwrap_or(Value::Array(Vec::new())))
            .map_err(|e| Error::Validation(format!("invalid resources array: {e}")))
    }

    pub async fn list_resource_templates(&mut self) -> Result<Vec<ResourceTemplateDefinition>> {
        let resp = self.request("resources/templates/list", Some(json!({}))).await?;
        let res: Value = serde_json::from_value(resp).map_err(|e| {
            Error::Validation(format!("invalid resources/templates/list response: {e}"))
        })?;
        serde_json::from_value(
            res.get("resourceTemplates").cloned().unwrap_or(Value::Array(Vec::new())),
        )
        .map_err(|e| Error::Validation(format!("invalid resourceTemplates array: {e}")))
    }

    pub async fn read_resource(&mut self, uri: &str) -> Result<ResourceContents> {
        let resp = self.request("resources/read", Some(json!({ "uri": uri }))).await?;
        let res: Value = serde_json::from_value(resp)
            .map_err(|e| Error::Validation(format!("invalid resources/read response: {e}")))?;
        let contents = res.get("contents").and_then(Value::as_array).ok_or_else(|| {
            Error::Validation("missing 'contents' array in resources/read response".into())
        })?;
        let first = contents.first().ok_or_else(|| {
            Error::Validation("empty 'contents' array in resources/read response".into())
        })?;
        serde_json::from_value(first.clone())
            .map_err(|e| Error::Validation(format!("invalid ResourceContents: {e}")))
    }

    pub async fn ping(&mut self) -> Result<()> {
        let _ = self.request("ping", None).await?;
        Ok(())
    }

    pub async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(id.into())),
            method: method.into(),
            params,
        };
        let resp = self.send_rpc(req).await?.ok_or_else(|| {
            Error::External("expected JSON-RPC response for request, got none".into())
        })?;
        if let Some(err) = resp.error {
            if err.code == JsonRpcError::FORBIDDEN {
                return Err(Error::Forbidden);
            }
            if err.code == JsonRpcError::RATE_LIMITED {
                let retry_after = err
                    .data
                    .as_ref()
                    .and_then(|d| d.get("retryAfterSecs"))
                    .and_then(Value::as_u64)
                    .unwrap_or(1);
                return Err(Error::RateLimited { retry_after_secs: retry_after });
            }
            return Err(Error::Validation(format!("JSON-RPC error {}: {}", err.code, err.message)));
        }
        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let req = JsonRpcRequest { jsonrpc: "2.0".into(), id: None, method: method.into(), params };
        let _ = self.send_rpc(req).await?;
        Ok(())
    }

    pub async fn send_rpc(&mut self, req: JsonRpcRequest) -> Result<Option<JsonRpcResponse>> {
        match &mut self.backend {
            ClientBackend::Http { client, url, token, session_id } => {
                let mut builder = client
                    .post(url.as_str())
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json, text/event-stream");
                if let Some(tok) = token.as_deref()
                    && !tok.trim().is_empty()
                {
                    builder = builder.bearer_auth(tok.trim());
                }
                if let Some(sid) = session_id.as_deref() {
                    builder = builder.header(MCP_SESSION_HEADER, sid);
                }
                let res =
                    builder.json(&req).send().await.map_err(|e| Error::External(e.to_string()))?;
                if let Some(sid) =
                    res.headers().get(MCP_SESSION_HEADER).and_then(|v| v.to_str().ok())
                {
                    *session_id = Some(sid.to_owned());
                }
                let status = res.status();
                if status == StatusCode::ACCEPTED || status == StatusCode::NO_CONTENT {
                    return Ok(None);
                }
                if status == StatusCode::UNAUTHORIZED {
                    return Err(Error::Unauthorized);
                }
                if status == StatusCode::FORBIDDEN {
                    return Err(Error::Forbidden);
                }
                if status == StatusCode::SERVICE_UNAVAILABLE {
                    return Err(Error::Unavailable("MCP unavailable".into()));
                }
                if !status.is_success() {
                    return Err(Error::External(format!("HTTP {status}")));
                }
                let bytes = res.bytes().await.map_err(|e| Error::External(e.to_string()))?;
                if bytes.is_empty() {
                    return Ok(None);
                }
                let rpc_resp: JsonRpcResponse = serde_json::from_slice(&bytes)
                    .map_err(|e| Error::Validation(format!("invalid JSON-RPC response: {e}")))?;
                Ok(Some(rpc_resp))
            }
            ClientBackend::Router {
                router,
                token,
                cookie,
                origin,
                host,
                connect_addr,
                session_id,
            } => {
                let payload = serde_json::to_vec(&req).map_err(|e| Error::Internal(Box::new(e)))?;
                let mut builder = Request::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .header(header::CONTENT_TYPE, "application/json");
                if let Some(tok) = token.as_deref()
                    && !tok.trim().is_empty()
                {
                    builder =
                        builder.header(header::AUTHORIZATION, format!("Bearer {}", tok.trim()));
                }
                if let Some(ck) = cookie.as_deref()
                    && !ck.trim().is_empty()
                {
                    builder =
                        builder.header(header::COOKIE, format!("{SESSION_COOKIE}={}", ck.trim()));
                }
                if let Some(h) = host.as_deref() {
                    builder = builder.header(header::HOST, h);
                }
                if let Some(o) = origin.as_deref() {
                    builder = builder.header(header::ORIGIN, o);
                }
                if let Some(sid) = session_id.as_deref() {
                    builder = builder.header(MCP_SESSION_HEADER, sid);
                }
                let mut http_req =
                    builder.body(Body::from(payload)).map_err(|e| Error::Internal(Box::new(e)))?;
                if let Some(addr) = *connect_addr {
                    http_req.extensions_mut().insert(ConnectInfo(addr));
                }
                let res = router
                    .clone()
                    .oneshot(http_req)
                    .await
                    .map_err(|e| Error::Internal(Box::new(e)))?;
                if let Some(sid) =
                    res.headers().get(MCP_SESSION_HEADER).and_then(|v| v.to_str().ok())
                {
                    *session_id = Some(sid.to_owned());
                }
                let status = res.status();
                if status == StatusCode::ACCEPTED || status == StatusCode::NO_CONTENT {
                    return Ok(None);
                }
                if status == StatusCode::UNAUTHORIZED {
                    return Err(Error::Unauthorized);
                }
                if status == StatusCode::FORBIDDEN {
                    return Err(Error::Forbidden);
                }
                if status == StatusCode::SERVICE_UNAVAILABLE {
                    return Err(Error::Unavailable("MCP unavailable".into()));
                }
                if !status.is_success() {
                    return Err(Error::External(format!("HTTP {status}")));
                }
                let bytes = res
                    .into_body()
                    .collect()
                    .await
                    .map_err(|e| Error::Internal(Box::new(e)))?
                    .to_bytes();
                if bytes.is_empty() {
                    return Ok(None);
                }
                let rpc_resp: JsonRpcResponse = serde_json::from_slice(&bytes)
                    .map_err(|e| Error::Validation(format!("invalid JSON-RPC response: {e}")))?;
                Ok(Some(rpc_resp))
            }
            ClientBackend::Stdio { reader, writer } => {
                let is_notification = req.id.is_none();
                let mut line =
                    serde_json::to_string(&req).map_err(|e| Error::Internal(Box::new(e)))?;
                line.push('\n');
                writer
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|e| Error::Internal(Box::new(e)))?;
                writer.flush().await.map_err(|e| Error::Internal(Box::new(e)))?;
                if is_notification {
                    return Ok(None);
                }
                let mut resp_line = String::new();
                let n = reader
                    .read_line(&mut resp_line)
                    .await
                    .map_err(|e| Error::Internal(Box::new(e)))?;
                if n == 0 {
                    return Err(Error::Unavailable("stdio session closed".into()));
                }
                let rpc_resp: JsonRpcResponse = serde_json::from_str(resp_line.trim())
                    .map_err(|e| Error::Validation(format!("invalid stdio JSON-RPC: {e}")))?;
                Ok(Some(rpc_resp))
            }
        }
    }
}

/// Forward newline-delimited JSON-RPC messages between `reader`/`writer` and a remote Streamable HTTP `/mcp` server.
pub async fn proxy_stdio_with_io<R, W>(
    mut reader: R,
    mut writer: W,
    server_url: &str,
    token: Option<String>,
) -> Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut client = McpClient::connect_http(server_url, token)?;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await.map_err(|e| Error::Internal(Box::new(e)))?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = JsonRpcResponse::err(
                    Value::Null,
                    JsonRpcError {
                        code: JsonRpcError::PARSE_ERROR,
                        message: format!("invalid JSON: {e}"),
                        data: None,
                    },
                );
                let mut out =
                    serde_json::to_string(&err_resp).map_err(|e| Error::Internal(Box::new(e)))?;
                out.push('\n');
                writer.write_all(out.as_bytes()).await.map_err(|e| Error::Internal(Box::new(e)))?;
                writer.flush().await.map_err(|e| Error::Internal(Box::new(e)))?;
                continue;
            }
        };
        if let Some(resp) = client.send_rpc(req).await? {
            let mut out = serde_json::to_string(&resp).map_err(|e| Error::Internal(Box::new(e)))?;
            out.push('\n');
            writer.write_all(out.as_bytes()).await.map_err(|e| Error::Internal(Box::new(e)))?;
            writer.flush().await.map_err(|e| Error::Internal(Box::new(e)))?;
        }
    }
    Ok(())
}

/// Run the process-level stdio ↔ Streamable HTTP proxy (`mbl-cli mcp --server <url>`).
pub async fn run_stdio_proxy(server_url: &str, token: Option<String>) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    proxy_stdio_with_io(stdin, stdout, server_url, token).await
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::*;

    #[test]
    fn classifies_lan_tailnet_and_wan_ips() {
        assert_eq!(classify_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))), NetworkClass::Lan);
        assert_eq!(classify_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50))), NetworkClass::Lan);
        assert_eq!(classify_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))), NetworkClass::Lan);
        assert_eq!(classify_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 5, 1))), NetworkClass::Lan);
        assert_eq!(classify_ip(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))), NetworkClass::Tailnet);
        assert_eq!(
            classify_ip(IpAddr::V4(Ipv4Addr::new(100, 100, 100, 100))),
            NetworkClass::Tailnet
        );
        assert_eq!(classify_ip(IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))), NetworkClass::Wan);
        assert_eq!(classify_ip(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))), NetworkClass::Wan);
        assert_eq!(classify_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)), NetworkClass::Lan);
        assert_eq!(
            classify_ip("fd7a:115c:a1e0::1".parse::<IpAddr>().unwrap()),
            NetworkClass::Tailnet
        );
        assert_eq!(classify_ip("2001:db8::1".parse::<IpAddr>().unwrap()), NetworkClass::Wan);
    }

    #[test]
    fn origin_validation_rules() {
        let mut h = HeaderMap::new();
        // No Origin, no Bearer -> false
        assert!(!validate_origin(&h, &[]));

        // No Origin, with Bearer -> true
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer secret-tok"));
        assert!(validate_origin(&h, &[]));

        // Bad Origin even with Bearer -> false
        h.insert(header::HOST, HeaderValue::from_static("localhost:8420"));
        h.insert(header::ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(!validate_origin(&h, &[]));

        // Configured origin -> true
        assert!(validate_origin(&h, &["https://evil.example".to_owned()]));

        // Same-origin -> true
        h.insert(header::ORIGIN, HeaderValue::from_static("http://localhost:8420"));
        assert!(validate_origin(&h, &[]));
    }
}
