//! Model Context Protocol (MCP) server, Streamable HTTP and stdio transports, and client helpers.

pub mod audit;
pub mod caller;
pub mod resources;
pub mod sanitize;
pub mod server;
pub mod tools;
pub mod transport;

pub use audit::{
    AUDIT_RETENTION_MS, McpAuditPruneHandler, REDACTED_PLACEHOLDER, extract_dry_run,
    is_sensitive_key, list_recent_audit, prune_expired_audit, record_tool_call, redact_args,
};
pub use caller::{Caller, McpRateLimiter, is_expensive_tool, resolve_caller};
pub use resources::{
    TEMPLATE_ITEM, TEMPLATE_JOB, URI_HW_REPORT, URI_SERVER_HEALTH, URI_SERVER_INFO,
    URI_SESSIONS_ACTIVE, list_resource_templates, list_resources, read_resource,
};
pub use sanitize::{
    MAX_SUMMARY_LEN, MAX_TITLE_LEN, is_bidi_control, sanitize_text, untrusted, untrusted_title,
};
pub use server::{
    AGENT_INSTRUCTIONS, CallToolResult, ContentBlock, GetPromptResult, InitializeResult,
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, ListToolsResult, MblMcp, MutationOpts,
    PROTOCOL_VERSION, PromptArgument, PromptDefinition, PromptMessage, PromptsCapability,
    RegisteredTool, ResourceContents, ResourceDefinition, ResourceTemplateDefinition,
    ServerCapabilities, ServerHandler, ServerIdentification, ToolAnnotations, ToolDefinition,
};
pub use tools::{all_tools, execute_tool};
pub use transport::{
    MCP_SESSION_HEADER, McpClient, McpConfig, McpState, NetworkClass, classify_ip,
    normalize_mcp_url, proxy_stdio_with_io, router, run_stdio_proxy, validate_origin,
};
