# Several MBL servers: remote control across servers, and clients that choose a server

The user has more than one MBL (a Raspberry Pi and a Windows PC). Goals:

1. **Remote control across servers.** From one MBL, see and control the players connected to
   another (pause, seek, volume, play something), and start a title from the other server's
   library on one of its players.
2. **Clients choose a server.** The web app, and later the Android and TV apps, know several
   servers and switch between them; each server stays the owner of its own library, users and
   sessions.

## Design: linked servers over MCP

Each MBL already exposes MCP tools for exactly this: `list_players`, `control_player`,
`play_on_player`, `search_media`, `get_item`, `list_sessions` (scopes `read` and `playback`).
They accept API tokens that are limited by scope and network and show up in the MCP audit log.
So server A links to server B by B's MCP address and a token made on B:

- **On B:** Administration → AI agents → new token, scopes `read` + `playback`, networks LAN
  (and tailnet if the servers reach each other over Tailscale). Nothing new to open: the same
  rules as for an AI agent, and B's admin can revoke it at any time.
- **On A:** Server settings → Linked servers → add (name, address, token). A stores the token
  encrypted-at-rest like the other secrets in `settings`, never returns it, and checks it with
  `list_players` when saved.
- **A as client:** `mbl_mcp::McpClient` (already used by `mbl-cli mcp` and tests) calls B's
  tools. A caches nothing but the last player list (a few seconds), so B's state is always B's.

Why not call B's REST API from the browser: browsers block cross-server requests unless every
server allows every other server's origin, and REST uses session logins, not scoped tokens. Why
not share users between servers: each server keeps its own accounts, permissions and age
limits; a link only grants what the token's scopes allow on B.

## Phases

1. **M1 – Linked servers.** Settings, storage, test, status (reachable, version, players). REST
   `GET/POST/DELETE /api/v1/system/linked-servers`, MCP exclusion (secrets).
2. **M2 – Remote control across servers.** The Remote page and "Play on device" list players
   from all linked servers, grouped by server; commands go to the right server. REST
   `GET /api/v1/linked-servers/{id}/players`, `POST …/players/{player}/command`.
3. **M3 – Play from another library.** Search on a linked server from Remote/"Play on device"
   and start a title there on one of its players (`search_media` + `play_on_player` on B). The
   player streams from B, so permissions, transcoding and history stay on B.
4. **M4 – Choose a server in the web app.** A server switcher in the user menu listing linked
   servers (and the server's own LAN/tailnet/public addresses from `/api/v1/system/endpoints`);
   choosing one opens that server's web app, where the user signs in with B's account. The
   same list is what the Android and TV apps show (they already find servers with mDNS
   `_mbl._tcp` and `/system/endpoints`); phase A0 of the Android plan reuses it.
5. **M5 – Later.** Show "Continue watching" from linked servers on the home page (read-only,
   via MCP `get_continue_watching` with a token made for that user), and casting a title from
   B to a Chromecast through A's Cast sender.

## Security notes

- The token for B lives on A; anyone who is admin on A can control B's players within the
  token's scopes. The link form says so, and B's audit log names the token on every call.
- Network limits on B's token keep a leaked token useless outside the LAN/tailnet.
- No forwarding of A's users to B: a user on A controlling B's players acts with the token's
  rights on B, so links are admin-only to create, and the player list on A is shown to admins
  and to users the admin allows (setting per link: "who may use this server's players").

## Done when

- From the Pi's web app, pause/seek a film playing on the Windows server's browser player and
  start a Windows-library title on it; the Windows audit log shows the calls.
- Revoking the token on B makes A show the link as failing within seconds.
- The web app's server switcher opens the other server; the Android plan points to this list.
