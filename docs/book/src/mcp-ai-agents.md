# Model Context Protocol (MCP) & AI Integration

MBL natively integrates the Anthropic **Model Context Protocol (MCP)**, allowing external AI assistants (Claude Desktop, Cursor, Antigravity, Open-WebUI) to manage your media server securely.

## Capabilities & Tool Scopes

MBL implements **78 distinct MCP tools** across scoped permission sets:

- **Read Tools (`Scope::Read`, 33 tools):** `get_server_info`, `search_media`, `browse_items`, `get_item`, `get_children`, `get_hubs`, `get_continue_watching`, `get_watch_history`, `list_sessions`, `search_guide`, `list_recordings`, etc.
- **Library Tools (`Scope::Library`, 38 tools):** Read tools plus `scan_library`, `refresh_metadata`, `create_collection`, `add_to_collection`, `create_smart_collection`.
- **Admin Tools (`Scope::Admin`, 78 tools):** Full server management including `create_user`, `update_user_permissions`, `create_invite`, `pair_worker`, `wake_worker`, `import_plex_history`, `import_jellyfin_history`, `create_backup`, `restart_server`, and `recover_recording` for saved failed DVR media with `dry_run` and `confirm`.

Internet discovery adds `search_online_sources`, `inspect_online_source` and `get_online_history`
to the read scope. Both require WebDrama to be enabled and reject restricted
profiles and private source addresses. Website text is untrusted external data.
Inspection refreshes a per-user metadata cache without adding library items or
updating playback history. See [WebDrama discovery](online-discovery.md).

## OAuth 2.1 PKCE Authentication

MBL implements RFC 7636 Proof Key for Code Exchange (PKCE) with SHA-256 challenges:
- Authorization Server metadata: `/.well-known/oauth-authorization-server`
- Protected Resource metadata: `/.well-known/oauth-protected-resource`
- Dynamic Client Registration: `POST /oauth/register`
- Interactive User Consent & Token Exchange: `POST /oauth/token`

## Connecting Claude Desktop

Add MBL to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "mbl": {
      "command": "mbl-mcp",
      "args": [
        "--server", "http://localhost:8420",
        "--token", "mbl_sec_admin_token_here"
      ]
    }
  }
}
```

Or connect directly via Server-Sent Events (SSE) or WebSocket streaming endpoints.
