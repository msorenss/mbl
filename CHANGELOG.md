# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Cargo workspace, tooling (`just`, CI) and crate skeletons.
- `mbl-server` boots with `/health` and `/api/v1/system/info`, layered configuration, and logging to stdout and a file.
- SQLite database with the initial schema, separate read and write pools, and a backup before migrations.
- `cargo xtask gen-media` synthetic test library generator.
- Built-in device capability profiles (`chrome`, `firefox`, `safari`, `edge`, `generic-hls`) with dynamic client profile merging (`mbl-playback`).
- Pure playback decision engine (`decide`) choosing direct play, direct stream (remux), audio-only transcode or video transcode with human-readable reasons.
- Direct-play streaming endpoint (`GET/HEAD /api/v1/stream/direct/{partId}`) with HTTP `Range` support (`206`/`416`), 6-hour HMAC `?token=` authentication, and library-root path traversal protection.
- Typed FFmpeg pipeline model and pure argument builder (`mbl-transcode`) supporting remux copy, AAC/AC3/Opus audio transcoding, software video encoding (`libx264`/`libx265`/`libsvtav1`), deinterlacing (`yadif`), text/bitmap subtitle burn-in, filter-graph escaping, and fMP4 HLS segment output.
- HLS transcode/remux session manager (`mbl-transcode`, `mbl-stream`) with upfront master/media playlist generation, `/api/v1/stream/hls/{session}/...` endpoints, seek-on-demand FFmpeg restarts, `SIGSTOP`/`SIGCONT` ahead-throttling, 60-second inactivity cleanup, disk quota enforcement, and session concurrency caps.
- External sidecar subtitle detection during library scan (`Movie.en.srt`, `.en.forced.srt`, `.eng.sdh.srt`) and subtitle streaming endpoint (`GET /api/v1/stream/subtitles/{partId}/{streamIndex}.{vtt,ass}`) with FFmpeg WebVTT conversion, raw ASS delivery, and disk caching.
- Playback session negotiation and watch state API (`POST /api/v1/playback`, `/api/v1/playback/{session}/progress`, `/api/v1/playback/{session}/stop`, and cascading `POST/DELETE /api/v1/items/{id}/watched`), resume offset persistence, `watch_history` recording, and automatic pausing of idle background jobs during active playback.
- Model Context Protocol (`mbl-mcp`) server bootstrap mounted at `/mcp` (Streamable HTTP) with `Origin` and WAN validation (`mcp.enabled`, `mcp.allow_wan`, `mcp.allowed_origins`), `mbl-cli mcp --server <url>` stdio proxy, and `just mcp-inspect`.
- Scoped API tokens (`POST/GET/DELETE /api/v1/auth/tokens`; scopes `read`, `playback`, `library`, `users`, `remote`, `livetv`, `admin`; optional expiry and allowed networks), per-caller MCP tool filtering, per-token rate limits, and an `mcp_audit` log with secret redaction and 90-day retention.
- Model Context Protocol tools v0 (21 tools across `system`, `libraries`, `media`, `watch`, `playback`, and `jobs`), static and templated resources (`mbl://server/info`, `mbl://server/health`, `mbl://sessions/active`, `mbl://items/{id}`, `mbl://jobs/{id}`), untrusted metadata hygiene (control/bidi character stripping, structuredContent isolation), and cursor pagination.
- MCP administration REST endpoints (`GET /api/v1/mcp/audit` with token/tool/outcome filters, `GET/PATCH /api/v1/mcp/status` with global kill-switch toggle, `POST /api/v1/mcp/snippets` generating VS Code/Claude Desktop/Cursor/stdio configuration, `GET /api/v1/mcp/presets` for the Token Wizard, and `GET /api/v1/auth/tokens?all=true` for admin token auditing).
- Web client bootstrap (`web/` workspace using Vite, SolidJS, `@solidjs/router`, `@tanstack/solid-query`, `@tanstack/solid-virtual`, `openapi-fetch`, `hls.js`, `@kobalte/core`, Biome, Vitest, typed API client generated via `pnpm gen:api`, auth guard, layout shell, dark theme, and `web-check` in `just check`).
- AI agent administration page (`web/src/pages/admin/AiAgents.tsx`) with Token Wizard, scope presets, one-time secret modal, copy-paste config snippets (VS Code, Claude Desktop, Cursor, stdio), active token revocation table, MCP audit log table with tool/outcome/token filters, and global MCP kill-switch toggle.
- Web MVP screens (`web/`): 4-step setup wizard, full-featured media player with dynamic `DeviceProfile` detection, direct play and HLS (`hls.js`), VTT subtitle switcher and progress reporting; home dashboard with horizontal hub rows (`Continue Watching`, `Up Next`, `Recently Added`); virtualized library poster grid with sort and unwatched filter; detail view with backdrop hero and episodes list; debounced instant search; and user administration.
- Packaging v0.1: Embedded production SPA client via `rust-embed` with SPA routing fallback; Linux Debian package configuration (`cargo-deb`) for amd64 and aarch64 (Raspberry Pi 5) with hardened systemd service unit, `postinst`/`prerm` lifecycle scripts and user/group setup; Windows distribution package structure with `README.txt` and FFmpeg setup instructions; and GitHub Actions release automation (`.github/workflows/release.yml`) building multi-platform release artifacts, checksums (`SHA256SUMS`), and draft GitHub releases on tags `v*`.

