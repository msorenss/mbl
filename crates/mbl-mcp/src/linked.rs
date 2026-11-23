//! Calls another MBL's MCP tools for linked servers (`mbl_service::linked_servers`).

use mbl_core::{Error, Result};
use mbl_service::linked_servers::LinkedServerClient;
use serde_json::Value;

use crate::transport::McpClient;

/// Opens an MCP session per call: links are used now and then, and a fresh session never
/// carries state from an earlier one.
#[derive(Debug, Default, Clone, Copy)]
pub struct McpLinkedClient;

#[async_trait::async_trait]
impl LinkedServerClient for McpLinkedClient {
    async fn call(&self, url: &str, token: &str, tool: &str, args: Value) -> Result<Value> {
        let mut client = McpClient::connect_http(url, Some(token.to_owned()))?;
        client.initialize().await.map_err(|e| {
            Error::Unavailable(format!("could not reach MBL at {url}: {}", short(&e.to_string())))
        })?;
        let result = client.call_tool(tool, args).await.map_err(|e| {
            // "validation: JSON-RPC error -32603: service unavailable (retry later): …" → the
            // other server's own reason.
            let text = e.to_string();
            let reason = text.rsplit(": ").next().unwrap_or(&text);
            Error::Unavailable(short(reason))
        })?;
        if result.is_error {
            let text: Vec<&str> = result.content.iter().map(|c| c.text.as_str()).collect();
            return Err(Error::External(short(&text.join(" "))));
        }
        Ok(result.structured_content.unwrap_or(Value::Null))
    }
}

fn short(text: &str) -> String {
    text.chars().take(300).collect()
}
