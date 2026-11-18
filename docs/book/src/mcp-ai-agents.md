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

The metadata documents contain absolute URLs. MBL builds them from `server.public_url` when it is set, otherwise from the address the client used. A request to `/mcp` without credentials, or with an invalid token, is answered with `401` and `WWW-Authenticate: Bearer resource_metadata="…/.well-known/oauth-protected-resource"`, so MCP clients that support OAuth can start the sign-in flow on their own. Set `server.public_url` when MBL runs behind a reverse proxy, so the URLs point to the public address.

## Connecting Claude Desktop and other stdio clients

Clients that start a local program use `mbl-cli`, which is installed next to `mbl-server` (`/usr/bin/mbl-cli` in the Debian package, `C:\Program Files\MBL\bin\mbl-cli.exe` in the Windows installer). Create a token under **Administration → AI Agents**; the wizard shows the finished configuration. For Claude Desktop, `claude_desktop_config.json` looks like this:

```json
{
  "mcpServers": {
    "mbl": {
      "command": "mbl-cli",
      "args": ["mcp", "--server", "http://192.168.1.50:8420"],
      "env": { "MBL_TOKEN": "mbl_…" }
    }
  }
}
```

Clients that speak Streamable HTTP (Cursor, VS Code) connect directly to `http://<server>:8420/mcp` with the header `Authorization: Bearer <token>`.
