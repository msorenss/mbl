//! Model Context Protocol (MCP) administrative services and configuration snippets.

use std::sync::{Arc, RwLock};

use mbl_core::{Actor, Error, Result};
use mbl_db::Db;
use mbl_db::repo::mcp::{AuditFilter, McpAuditEntry};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Configuration for the Model Context Protocol (MCP) subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    pub allow_wan: bool,
    pub allowed_origins: Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self { enabled: true, allow_wan: false, allowed_origins: Vec::new() }
    }
}

/// Administrative status and toggle for the MCP subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub allow_wan: bool,
    pub allowed_origins: Vec<String>,
}

/// Request payload to update MCP status (e.g. kill-switch toggle).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMcpStatusRequest {
    pub enabled: bool,
}

/// Request to generate client configuration snippets for connecting an AI agent.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpSnippetsRequest {
    pub server_url: Option<String>,
    pub token: String,
}

/// Ready-to-copy configuration snippets for popular AI agents and IDEs.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigSnippets {
    pub vscode: String,
    pub claude_desktop: String,
    pub cursor: String,
    pub stdio: String,
}

/// Preset permission bundle for the Token Wizard.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scopes: Vec<String>,
}

/// Generate client configuration snippets for VS Code, Claude Desktop, Cursor, and stdio.
pub fn generate_snippets(server_url: Option<&str>, token: &str) -> McpConfigSnippets {
    let base = server_url
        .unwrap_or("http://localhost:8420")
        .trim_end_matches('/')
        .trim_end_matches("/mcp")
        .trim_end_matches('/');

    let mcp_url = format!("{base}/mcp");

    let vscode_obj = serde_json::json!({
        "servers": {
            "mbl": {
                "type": "http",
                "url": mcp_url,
                "headers": {
                    "Authorization": format!("Bearer {token}")
                }
            }
        }
    });

    let claude_obj = serde_json::json!({
        "mcpServers": {
            "mbl": {
                "command": "mbl-cli",
                "args": ["mcp", "--server", base],
                "env": {
                    "MBL_TOKEN": token
                }
            }
        }
    });

    let cursor_obj = serde_json::json!({
        "mcpServers": {
            "mbl": {
                "url": mcp_url,
                "headers": {
                    "Authorization": format!("Bearer {token}")
                }
            }
        }
    });

    let stdio_cmd = format!(r#"MBL_TOKEN="{token}" mbl-cli mcp --server "{base}""#);

    McpConfigSnippets {
        vscode: serde_json::to_string_pretty(&vscode_obj).unwrap_or_default(),
        claude_desktop: serde_json::to_string_pretty(&claude_obj).unwrap_or_default(),
        cursor: serde_json::to_string_pretty(&cursor_obj).unwrap_or_default(),
        stdio: stdio_cmd,
    }
}

/// Built-in scope presets for the AI agent Token Wizard.
pub fn mcp_presets() -> Vec<McpPreset> {
    vec![
        McpPreset {
            id: "read_only".into(),
            name: "Read-only".into(),
            description: "Can inspect server health, logs, libraries, media, and search".into(),
            scopes: vec!["read".into()],
        },
        McpPreset {
            id: "viewer_playback".into(),
            name: "Viewer + playback".into(),
            description: "Read access plus session previews and marking items watched/unwatched"
                .into(),
            scopes: vec!["read".into(), "playback".into()],
        },
        McpPreset {
            id: "library_manager".into(),
            name: "Library manager".into(),
            description: "Read access plus triggering library scans and metadata refreshes".into(),
            scopes: vec!["read".into(), "library".into()],
        },
        McpPreset {
            id: "full_admin".into(),
            name: "Full admin".into(),
            description: "All scopes (admin, users, library, playback, read, remote, livetv)"
                .into(),
            scopes: vec![
                "read".into(),
                "playback".into(),
                "library".into(),
                "users".into(),
                "remote".into(),
                "livetv".into(),
                "admin".into(),
            ],
        },
    ]
}

/// Administrative service for MCP configuration, kill-switch, audit view, and snippets.
#[derive(Debug, Clone)]
pub struct McpService {
    db: Db,
    config: Arc<RwLock<McpConfig>>,
}

impl McpService {
    pub fn new(db: Db, config: Arc<RwLock<McpConfig>>) -> Self {
        Self { db, config }
    }

    pub fn db(&self) -> Db {
        self.db.clone()
    }

    pub fn config_handle(&self) -> Arc<RwLock<McpConfig>> {
        self.config.clone()
    }

    /// List audit rows with optional filtering (admin only).
    pub async fn list_audit(
        &self,
        actor: &Actor,
        filter: &AuditFilter,
    ) -> Result<Vec<McpAuditEntry>> {
        actor.require_admin()?;
        mbl_db::repo::mcp::list_audit_filtered(&self.db, filter).await
    }

    /// Get current MCP subsystem status (admin only).
    pub fn get_status(&self, actor: &Actor) -> Result<McpStatus> {
        actor.require_admin()?;
        let cfg =
            self.config.read().map_err(|_| Error::Internal("mcp config lock poisoned".into()))?;
        Ok(McpStatus {
            enabled: cfg.enabled,
            allow_wan: cfg.allow_wan,
            allowed_origins: cfg.allowed_origins.clone(),
        })
    }

    /// Toggle MCP kill switch (admin only).
    pub fn set_enabled(&self, actor: &Actor, enabled: bool) -> Result<McpStatus> {
        actor.require_admin()?;
        let mut cfg =
            self.config.write().map_err(|_| Error::Internal("mcp config lock poisoned".into()))?;
        cfg.enabled = enabled;
        Ok(McpStatus {
            enabled: cfg.enabled,
            allow_wan: cfg.allow_wan,
            allowed_origins: cfg.allowed_origins.clone(),
        })
    }

    /// Generate connection snippets for AI agent clients.
    pub fn generate_snippets(
        &self,
        _actor: &Actor,
        server_url: Option<&str>,
        token: &str,
    ) -> Result<McpConfigSnippets> {
        Ok(generate_snippets(server_url, token))
    }

    /// Return available scope presets for the Token Wizard.
    pub fn presets(&self, _actor: &Actor) -> Vec<McpPreset> {
        mcp_presets()
    }
}
