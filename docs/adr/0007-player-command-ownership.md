# ADR 0007: Player ownership and command delivery

Status: Accepted (2026-10-28)

Players registered over HTTP belong to the authenticated user and authentication
session. Only that session may heartbeat or open its command WebSocket. Users
can list/control their own players; administrators can inspect/control players.
Registration cannot take over another session's ID. Each Cast sender uses a
random player ID, separate from Google's shared receiver session identifier.

Commands require a live bounded per-player channel; an offline player returns
503. A 204 acknowledges queueing, not successful receiver execution. WebSocket
closure removes presence, heartbeats expire after 60 seconds, and authentication
revocation closes the socket on the next 20-second check. Reconnection registers
again. No cross-user sharing is implied by possession of a player ID.

Google Cast negotiates the conservative chromecast profile after receiver
selection. The server returns MIME, part ID and signed external caption URLs.
Configured public_url is preferred, and loopback media URLs are rejected by the
sender. Physical receiver compatibility and routing still require acceptance tests.
