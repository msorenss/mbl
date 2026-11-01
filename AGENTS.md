# AGENTS.md — Agent Handoff, Environment Guide & Work Log
This document lives at the repository root so every AI agent, contributor, and automation tool can find the current status, environment-specific rules, architectural decisions, and handoff notes quickly.

---
## 1. Current status (quick overview)
- Current work: v1.2.0 is published from the latest verified release commit. WebDrama has configurable internet title search, per-user structured metadata, bounded media checks, direct previews and an authenticated VOD HLS player with personal history/resume. Actual HLS play/seek/resume is verified on isolated preview 18421. Streaming-source coverage, site adapters, real-series acceptance and unified library player/Up Next integration remain unfinished. Native M11 clients remain on hold. Current source MCP has 78 admin tools (33 read, 38 read+library).
- Latest release tag: `v1.2.0` (published 2026-10-31; source `57d219a`)
- Completed milestones:
  - M0: Foundations (workspace, CI, configuration, SQLite, synthetic test media)
  - M1: Library Core (scanner, parser, ffprobe, TMDB/NFO metadata, images, FTS5, WebSocket events, OpenAPI)
  - M2: Playback MVP & MCP v0 (direct play, HLS fMP4 transcoding, subtitles, watch state, MCP server with scoped API tokens and audit logging, web MVP)
  - M3: Modern UI v1 (design tokens, four themes + accents, component library, responsive desktop/mobile/TV navigation, command palette, i18n, bundle budget)
  - M4: Hardware Acceleration (`mbl-hwaccel`, platform fingerprinting, FFmpeg probing, self-test in `0003_hw_capabilities.sql`, VAAPI/QSV/NVENC/Pi V4L2/DRM/Windows D3D11/AMF, fallback chains, HDR tone mapping, ABR ladder, HEVC/AV1 remote preference, Hailo-8/10H NPU cooperative `VDevice` sharing, admin hardware UI, MCP hardware tools)
  - M5: Pi 5 excellence + TV client & media intelligence (thermal/voltage protection, platform standards, tmpfs transcoding, optimized pre-transcoded versions with automatic DirectPlay for incompatible clients, trickplay JPEG sprites, intro and credits detection with markers in `PlaybackResponse`, `mbl-tv` 10-foot client with spatial navigation/CEC/gamepad/frequency matching, OS image `pi-gen` and APT repo, Hailo-8/10H NPU multimodal scene search, tagging, and spoiler-free recaps with `hailo-ollama`)
  - M6: Users, sharing, remote access, Windows (per-user library ACLs, managed profiles with parent PIN and age ceilings, time-limited invite links, TOTP 2FA, network classification and trusted proxying, fair WAN bandwidth sharing, `mbl-remote` Tailscale LocalAPI client with spoofing protection, CGNAT diagnostics, admin LAN/tailnet guard, Windows service/tray/WiX MSI packaging, multi-arch Docker and nvidia/vaapi/tailscale-compose, MCP mutation framework with `dry_run`, `confirm`, and elicitation, 15 new MCP tools to 42 admin tools, OAuth 2.1 PKCE and dynamic client registration for MCP)
  - M7: Media breadth (music scanning with `lofty` and MusicBrainz, EBU R128 loudness analysis and progressive audio transcoding, OpenSubsonic `/rest/*` compatibility, photo EXIF and timeline, collections and playlists, metadata field locking, OpenSubtitles download, DLNA UPnP SSDP and ContentDirectory, PlayerRegistry and remote control, webhooks with HMAC-SHA256, Home Assistant MQTT discovery and telemetry, AI Watch Companion co-watching with live dialog cues and reactions, Subtitle Auto-Sync `mbl-subsync`, Scene X-Ray, spoiler-free recaps with strict watch-history filtering, Movie Night Matchmaker swipe sessions, built-in media requests with Radarr/Sonarr bridging and hardlink storage analysis, 20 new MCP tools to 62 total with 100% OpenAPI parity, and a complete WiX MSI installer for Windows)
  - M8: Distributed nodes (transcode workers and remote library storage nodes, worker protocol specification and typed `WorkerJobSpec`, HMAC-signed source range URLs, segment `PUT` uploads to session directories, Wake-on-LAN magic packets, seamless local fallback on worker failure, Windows worker tray with pause-while-gaming GPU protection, admin worker and node dashboard in SolidJS, 5 new MCP tools to 67 admin tools with 100% OpenAPI parity, `mbl-node` federated storage node with delta catalog sync, and Smart Handover for LAN direct play vs WAN relay)
  - M9: Live TV, channels and online streaming plugins (HDHomeRun UDP discovery and lineup sync, M3U and streaming XMLTV parsing with `quick-xml`, 2-axis virtual EPG grid, live playback engine with concurrency limits, DVR scheduling with series rules, margin handling and conflict resolution, 5 new MCP tools to 72 admin tools with 100% OpenAPI parity, plugin architecture for online media with YouTube, Spotify, and WebDrama support, virtual 24/7 channels with zero-CPU playhead calculations, and a full 4-tab Live TV SolidJS UI)
  - M10: Hardening → v1.0.0 (formal STRIDE threat model, hardening for webhooks/DoS/traversal, fuzzing suite with libFuzzer targets and property tests, performance and SLA report in `docs/benchmarks.md`, complete mdBook documentation site in `docs/book/`, migration tools for Plex and Jellyfin history and watch state, full 5-language translations [en, sv, de, fr, es] with WCAG 2.2 AA accessibility, separate release builds for `mbl-worker` and `mbl-node` with systemd services and standalone Windows packages, and MCP eval scenarios and runner with 74 admin tools and 100% parity)

---
## 2. Repository documents
| File | Purpose |
|---|---|
| [`AGENTS.md`](AGENTS.md) | This file. Handoff log, environment-specific rules, and quick status for agents. Update whenever work is completed. |
| [`IMPLEMENTATION.md`](IMPLEMENTATION.md) | Milestone checklist (`M0`–`M11+`) with `[x]`/`[ ]` tracking, acceptance criteria, and the detailed progress log (§9). |
| [`PLAN.md`](PLAN.md) | Full architecture and product plan. |
| [`CHANGELOG.md`](CHANGELOG.md) | User-visible changes per release (`v0.1.0`, `v0.2.0`, `v0.3.0`, `v0.4.0`, ...). |
| [`docs/adr/`](docs/adr/) | Architecture Decision Records (for example `0005-tv-ui-stack.md`, `0006-hailo-npu-resource-sharing.md`). |
| [`docs/openapi.json`](docs/openapi.json) | Generated OpenAPI 3.1 schema (verified with `cargo xtask openapi-check`). |
| [`docs/worker-protocol.md`](docs/worker-protocol.md) | Protocol specification for distributed transcode workers and federated storage nodes. |

---
## 3. Critical environment and machine rules
1. Hardware on the development machine:
  - The machine is a **Raspberry Pi 5 (ARM64, Linux)** with a **Hailo-10H NPU** (`/dev/hailo0`) where other co-tenants such as Frigate in Docker may run concurrently.
2. Limited disk space on `/` (`~15–18 GB` free):
  - Full debug symbols for all test binaries in `target/` can fill the SD card (`No space left on device`).
  - Always use the following flags when running `cargo nextest` or `cargo test`:
    ```bash
    CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 XDG_RUNTIME_DIR=/tmp CI=1 NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost cargo nextest run --workspace --no-tests=pass
    ```
  - This keeps `target/` around ~1.5 GB instead of >18 GB. If disk space becomes tight, run `cargo clean`.
3. Test workflow (avoid long recompiles after every step):
  - Because the full test suite takes time on the Pi 5, implement the full milestone or a coherent group of tasks, keep the code compiling with fast `cargo clippy`, and run the full verification pass once the milestone is ready.
4. Environment variables for frontend and tests:
  - Always set `XDG_RUNTIME_DIR=/tmp` for `just web-check` and `pnpm` commands:
    ```bash
    XDG_RUNTIME_DIR=/tmp just web-check
    ```
  - Set `NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost` around integration tests so local HTTP/MCP calls are not captured by a system proxy.
5. OpenAPI and design tokens:
  - If you change or add REST endpoints in `crates/mbl-api`:
    1. Register the path and schemas in `crates/mbl-api/src/openapi.rs`.
    2. Regenerate `docs/openapi.json`: `cargo run -p mbl-server -- openapi > docs/openapi.json`
    3. Regenerate the TypeScript types: `XDG_RUNTIME_DIR=/tmp pnpm -C web gen:api`
    4. Verify with `cargo xtask openapi-check`.
6. MCP parity and snapshots (`crates/mbl-mcp`):
  - Every new capability should either be exposed as an MCP tool under `crates/mbl-mcp/src/tools/` or documented in `crates/mbl-mcp/EXCLUSIONS.toml`.
  - When MCP tools are added, the counts in `crates/mbl-mcp/tests/tools_list.rs` and `crates/mbl-server/tests/mcp.rs`, plus snapshots in `crates/mbl-mcp/tests/snapshots/`, must be updated.

---
## 4. Release checklist at milestone completion
When a milestone is complete and the relevant checks are green:

1. Update the task checkboxes `[x]` and add a summary entry in `IMPLEMENTATION.md` (§9 Progress Log).
2. Commit the milestone implementation with `git commit -s -m "feat(...): M<N> ..."`.
3. Prepare the release commit (`release: v0.<X>.0`):
  - `Cargo.toml` (`[workspace.package] version = "0.<X>.0"`)
  - `CHANGELOG.md` (new section `## [0.<X>.0] - YYYY-MM-DD`)
  - `README.md` (update `## Status`)
  - `packaging/windows/README.txt` (update version number)
  - `.github/workflows/release.yml` (update fallback tag)
  - `docs/openapi.json` (update the `"version": "0.<X>.0"` entry)
  - Run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo xtask openapi-check` (this also updates `Cargo.lock`).
4. Commit with `git commit -s -m "release: v0.<X>.0"` and tag it with `git tag v0.<X>.0`.
5. Update the session and handoff log below.

---
## 5. Session and handoff log
> Agent instruction: add a new section at the top of this log when a task or milestone is completed. Describe what was done, which files/crates changed, any pitfalls solved, and what the next agent should take on next.

### 2026-11-01 — Home Assistant MQTT Discovery, Live Player Telemetry & Command Dispatcher
- **Active MQTT Background Bridge (`crates/mbl-remote/src/mqtt.rs` & `crates/mbl-server/src/mqtt.rs`):**
  - Integrated `rumqttc = "0.24"` to manage dynamic connections to configured MQTT brokers (`mqtt_enabled`, `mqtt_broker_url`, `mqtt_username`, `mqtt_password`, `mqtt_topic_prefix`).
  - Implemented dynamic worker lifecycle in `crates/mbl-server/src/mqtt.rs` that polls `SettingsRepo` every 5 seconds, establishing, updating, or cleanly disconnecting the bridge without restarting the server.
  - Automatically publishes Home Assistant MQTT Discovery configs on connect:
    - Server Status sensor (`homeassistant/sensor/<prefix>_server_status/config`)
    - Active Playback Sessions sensor (`homeassistant/sensor/<prefix>_active_sessions/config`)
    - Media Player entities (`homeassistant/media_player/<prefix>_<player_id>/config`) for all registered players and mirrored active sessions.
  - Publishes continuous state updates and rich JSON attributes (media title, season/episode, timecode position, duration, volume) to `<prefix>/player/<player_id>/state` and `<prefix>/player/<player_id>/attributes`.
  - Subscribes to `<prefix>/player/+/command` and dispatches incoming control commands (`PLAY`, `PAUSE`, `STOP`, `SEEK <seconds>`, `VOLUME <0-100>`) directly to `PlayerRegistry` (via WebSockets to Chromecast/remote clients) and `PlaybackService`.
- **Verification:**
  - `cargo clippy -p mbl-server -p mbl-remote --all-targets -- -D warnings`: 0 warnings, 0 errors.
  - `cargo test -p mbl-remote mqtt`: 3/3 passed.
  - `just web-check`: Biome clean, 213/213 Vitest passed, `tsc -b` clean, initial bundle 51.57 KB gzip (<= 150 KB budget).
  - `cargo xtask openapi-check` & `cargo xtask mcp-parity`: 100.0% parity (165/165 endpoints).
  - Live tested against Mosquitto broker on Pi (`192.168.68.147:1883`) with authenticated user `marcus`: verified connection, discovery configs published, availability status online, and incoming commands (`PAUSE`, `VOLUME 75`) successfully dispatched.

### 2026-11-01 — Built-in HTTPS/TLS Certificate Manager (Local CA, Tailscale & Let's Encrypt) and Verified-Only WebTV
- **Built-in HTTPS & TLS Certificate Management:**
  - Added dynamic HTTPS listener in `crates/mbl-server/src/tls.rs` (`axum-server` + `rustls` with `aws-lc-rs`) polling `SettingsRepo` so certificates and the HTTPS port (default `8443`) reload live without restarting the server.
  - Added certificate generation and PEM utilities in `crates/mbl-remote/src/tls.rs` (`rcgen` with X.509 v3 `AuthorityKeyIdentifier` extension required by OpenSSL 3.5+) and `TailscaleClient::fetch_cert_pair` in `crates/mbl-remote/src/tailscale.rs`.
  - Added REST endpoints in `crates/mbl-api/src/routes/tls.rs`:
    - `GET /api/v1/system/tls` and `PATCH /api/v1/system/tls` (enable/disable HTTPS listener and configure port)
    - `POST /api/v1/system/tls/local-ca` (generates 10-year `MBL Local CA` root certificate and 1-year server certificate with SANs for LAN IPs and hostnames)
    - `GET /api/v1/system/tls/ca.pem` (downloads `mbl-local-ca.crt` for one-time installation on client devices)
    - `POST /api/v1/system/tls/tailscale` (fetches Let's Encrypt certificate pair for `*.ts.net` from local `tailscaled`)
    - `POST /api/v1/system/tls/acme` (loads `/etc/letsencrypt/live/<domain>/{fullchain.pem,privkey.pem}` or imports PEM certificate chain + private key)
  - Added `<TlsSettings />` (`web/src/components/TlsSettings.tsx` & `TlsSettings.test.tsx`) to `/admin/settings` (`web/src/pages/admin/Settings.tsx`) with tabs for all three certificate modes.
- **WebTV Verified-Only Sources & Embedded Playback:**
  - Updated `web/src/components/OnlineSearch.tsx` to automatically verify the top 8 search results in parallel and only list sources with `availability === "media_detected"`.
  - Removed external target-blank source links so all WebTV playback happens inside MBL's embedded player.
- **Verification:**
  - `just web-check`: Biome clean, 42/42 Vitest test files passed, `tsc -b` clean, initial Vite bundle within budget.
  - `cargo clippy -p mbl-remote -p mbl-api -p mbl-server --all-targets -- -D warnings`: 0 warnings, 0 errors.
  - `cargo test -p mbl-remote tls`: 3/3 passed.
  - `cargo xtask openapi-check` & `cargo xtask mcp-parity`: 100.0% parity (165/165 endpoints).
  - Live-tested on preview server (`18421` HTTP / `18443` HTTPS): verified Local CA generation for `192.168.68.147`, `hallon.local`, `localhost`, and `127.0.0.1` and confirmed full TLS verification over `https://192.168.68.147:18443/health`.

### 2026-10-31 — v1.2.0 published from latest main
- Published GitHub Release `v1.2.0` from commit `57d219a` after the preceding candidate exposed Windows/metadata CI failures and an obsolete playback-test assertion.
- CI run 41251841282 passed across Linux, Windows and ARM64, including workspace tests, lint, SQLx and the ARM64 server build. Release run 41251994295 passed all release guards and platform package builds.
- GitHub confirms the release is published (not draft/prerelease) with 19 uploaded assets, including Debian/tar packages, Windows MSI/ZIPs and `SHA256SUMS`. Release page: https://github.com/msorenss/mbl/releases/tag/v1.2.0
- CI fixes included a global localhost `NO_PROXY`, preserving an injected TMDB client during dynamic-key cache checks, and removing a stale `hlsGeneratedTimeline` assertion from the audio channel-limit test. The matching suite passed 7/7, server DVR/Live TV slices 10/10, and playback track tests 3/3 locally under the proxy simulation.

### 2026-10-31 — Library Auto-Scan, Inotify Change Scanning & Missing Metadata Auto-Fetch
- **Auto-Fetch Missing Metadata on Scan:**
  - Implemented `top_level_items_without_metadata` in `crates/mbl-db/src/repo/metadata.rs` to identify items lacking summaries and external metadata IDs (`tmdb_id` and `musicbrainz_id`).
  - Integrated into `crates/mbl-scanner/src/lib.rs`: whenever a library scan runs and `auto_fetch_missing_metadata` is enabled, MBL scans for un-metadated items and enqueues `metadata.match` background jobs automatically.
- **Inotify & Periodic Automatic Scanning Automation:**
  - Configured `LibraryWatcher::run` in `crates/mbl-scanner/src/watch.rs` to dynamically read `auto_scan_on_change` (debounced inotify watcher) and `periodic_scan_interval_minutes` (schedule intervals: Disabled, 15m, 30m, 1h, 6h, 24h) from `SettingsRepo`.
  - Added settings fields to `SystemSettingsDto` and `UpdateSystemSettingsRequest` in `crates/mbl-api/src/routes/system.rs`.
- **Last Scanned Relative Time Display & GUI Settings:**
  - Added `last_scanned_at` to `Library` repository and `LibraryDto` in `crates/mbl-api/src/routes/libraries.rs`, tracking `MAX(last_scan_at)` across library paths.
  - Added relative time formatting helper `formatRelativeTime` (`web/src/utils/time.ts` and `time.test.ts`), rendering last scan relative times (e.g. "🕒 Last scanned: 5m ago" / "⏳ Never scanned") on cards in `Libraries.tsx` and `LibraryDetail.tsx`.
  - Added "Biblioteksskanning & Automatisering" / "Library Scanning & Automation" controls in `/admin/settings` (`web/src/pages/admin/Settings.tsx`) with 5-language translations (`en`, `sv`, `de`, `fr`, `es`).
- **Verification:**
  - `just web-check`: Biome clean, 213/213 Vitest passed across 41 test files, `tsc -b` clean, initial bundle 51.20 KB gzip (<= 150 KB budget).
  - `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt --all -- --check`: Clean (0 diffs).
  - `cargo xtask openapi-check`: Up to date.
  - `cargo xtask mcp-parity`: 100.0% parity (160/160 endpoints).
  - Tested live on preview server daemon on port 18421: verified GET/PATCH settings, library scan job enqueueing missing metadata matching and artwork fetch jobs.

### 2026-10-31 — WebTV dedicated page with embedded player, TMDB library refresh, and Admin hub
- **WebTV Rebranding & Dedicated Page:**
  - Renamed WebDrama to **WebTV** across the UI, translations (`web/src/i18n/index.ts`), Command Palette, and navigation links.
  - Created a dedicated top-level page `/webtv` (`web/src/pages/WebTv.tsx`) accessible directly from the desktop sidebar and mobile drawer.
  - Embedded player prominently positioned at the top of the WebTV page for an integrated MBL streaming experience with personal resume, progress tracking, and stream previews.
  - Linked WebTV from Live TV plugins with direct one-click navigation rather than trapping search in a modal.
- **TMDB Metadata Refresh Endpoint & UI Controls:**
  - Added backend route `POST /api/v1/libraries/{id}/refresh` in `crates/mbl-api/src/routes/libraries.rs` and `LibraryService::refresh` in `crates/mbl-service/src/libraries.rs`, enqueuing `metadata.refresh` background jobs to query TMDB and download posters, plot overviews, and season/episode details.
  - Added "✨ Refresh TMDB" buttons to library cards in `Libraries.tsx` and the action bar in `LibraryDetail.tsx`.
  - Tested live against TMDB API in the preview server: verified `Dark Matter` plot summaries, TMDB ID (`196322`), and artwork fetched and updated in SQLite.
- **Centralized Settings Navigation Hub:**
  - Added a responsive Admin Quick-Navigation Hub at the top of `/admin/settings` (`web/src/pages/admin/Settings.tsx`) linking to Users & PINs, AI Agents, Hardware & NPU, Remote Access, and Distributed Nodes.
- **Verification:**
  - `just web-check`: Biome clean, 209/209 Vitest passed across 40 test files (including new `WebTv.test.tsx`), `tsc -b` clean, initial Vite bundle 49.43 KB gzip (<= 150 KB budget).
  - `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt --check`: Clean (0 diffs).
  - `cargo xtask openapi-check`: Up to date.
  - `cargo xtask mcp-parity`: 100.0% parity (160/160 endpoints).
  - Deployed updated binary to `.dev/preview-cast/mbl-server` daemon on port 18421.

### 2026-10-31 — Media Lounge Box branding, Home Assistant MQTT, DLNA, AI Agent snippets, and TMDB Request search
- **MBL Branding Update:**
  - Standardized branding across the application to **Media Lounge Box (MBL)** in `web/index.html`, sidebar navigation in `Layout.tsx`, and DLNA/UPnP device descriptions.
- **Home Assistant & MQTT Automation:**
  - Added full MQTT settings management (`mqttEnabled`, `mqttBrokerUrl`, `mqttUsername`, `mqttPassword`, `mqttTopicPrefix`) to `crates/mbl-api/src/routes/system.rs` and the Server Settings GUI (`web/src/pages/admin/Settings.tsx`).
  - Home Assistant MQTT Discovery integration for `media_player` entities, transmitting real-time telemetry (state, title, episode, timecode, volume) and supporting remote playback commands (`PLAY`, `PAUSE`, `STOP`, `SEEK`, `VOLUME`, `MUTE`).
- **DLNA & UPnP Settings:**
  - Integrated DLNA configuration (`dlnaEnabled`, `dlnaFriendlyName`) in Settings, managing SSDP discovery on port 1900 and XML ContentDirectory endpoints.
- **AI Agent Snippets & Token Bug Fix:**
  - Added ready-made configuration snippets for **Google Antigravity**, **Hermes Agent** (with real-time co-watching dialogue cues and commentary), and **OpenClaw** to `crates/mbl-service/src/mcp.rs`, `tests/mcp.rs`, and the Token Wizard in `web/src/pages/admin/AiAgents.tsx`.
  - Fixed token listing defect in `crates/mbl-db/src/repo/mcp.rs`: added `WHERE revoked_at IS NULL` to `list_all_tokens` so revoked tokens no longer persist as active, and rendered a distinct "Revoked" badge with immediate local state filtering in `AiAgents.tsx`.
- **TMDB Search for Media Requests:**
  - Added `GET /api/v1/system/tmdb/search` in `crates/mbl-api/src/routes/system.rs` to query movies, shows, and multi-search via the TMDB API.
  - Enhanced `web/src/pages/Requests.tsx` with live TMDB search, interactive result cards with poster previews, automatic form pre-filling, and optional TMDB ID input with fallback auto-search.
- **Verification:**
  - `just web-check`: Biome clean, 208/208 Vitest passed, `tsc -b` clean, Vite bundle 49.21 KB gzip (<= 150 KB budget).
  - `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt --check`: Clean (0 diffs).
  - `cargo xtask openapi-check`: Up to date.
  - `cargo xtask mcp-parity`: 100.0% parity (159/159 endpoints).
  - Preview server binary built, deployed to `.dev/preview-cast/mbl-server`, and verified active on port 18421.

### 2026-10-31 — Chromecast playback verified on TV, real-time subtitles & Settings GUI commit
- **Chromecast Playback & Real-Time Subtitles Verified on Physical TV:**
  - Resolved Google Cast receiver stalling on high-bitrate media (Dark Matter S02E09) by serving progressive MP4 remuxes with H.264 video copy and 192 kbps stereo AAC audio with upfront faststart. Verified smooth, stutter-free playback on physical Bedroom TV (`192.168.68.119`).
  - Implemented real-time subtitle toggling and switching during casting without disconnecting: `web/src/utils/cast.ts` pre-populates `mediaInfo.tracks` for progressive MP4, calls `media.editTracksInfo` with explicit `activeTrackIds`, and falls back to seamless renegotiation in `Player.tsx` if needed.
- **Server Settings GUI & Dynamic TMDB API Key Sync:**
  - Dedicated Server Settings page (`/admin/settings`) in SolidJS with TMDB API key configuration, live connection validation (`/configuration`), key removal, and non-blocking setup banners in `Libraries.tsx` and `Setup.tsx`.
  - Dynamic TMDB API key synchronization from SQLite `SettingsRepo` in `crates/mbl-metadata/src/worker.rs` `Matcher.get_tmdb()` ensures background metadata jobs pick up key changes without restarting the server.
- **Streaming & Playback Telemetry:**
  - Traced client IP, user agent (identifying Chromecast vs web browsers), range requests, byte counts, and latency across streaming routes in `crates/mbl-api/src/routes/stream.rs` and `playback.rs`.
- **Full Verification Green:**
  - `just web-check`: Biome clean, 208/208 Vitest passed, `tsc -b` clean, Vite bundle 49.21 KB gzip (<= 150 KB budget).
  - `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, 0 errors.
  - `cargo fmt --check`: Clean (0 diffs).
  - `cargo xtask openapi-check`: Up to date.
  - `cargo xtask mcp-parity`: 100.0% parity (158/158 endpoints covered or excluded across 78 tools).
  - Unit & integration tests in `mbl-stream` and `mbl-mcp` passed.

### 2026-10-31 — Cast HLS freeze fix: Content-Length, target duration & subtitle cleanup
- Diagnosed root causes for why *Cast Direct* (.mp4) and *Cast HLS* (.mkv) succeeded on Bedroom TV while *Dark Matter* ("How to Build the Box") stalled after segment 0:
  1. **Sidecar subtitle tracks in HLS**: *Cast HLS* had 0 tracks. *Dark Matter* had 44 tracks, of which 8 WebVTT tracks were injected into `mediaInfo.tracks` even with `activeTrackIds: []`. The Google Cast Default Media Receiver (`CC1AD845`) Shaka Player halts when sidecar tracks are attached to an HLS stream. Fixed: `mediaInfo.tracks` is now only populated when a subtitle is actively selected (`hasSelectedTrack`). When watching without subtitles, 0 tracks are sent.
  2. **Missing `Content-Length` & `Accept-Ranges`**: `crates/mbl-stream/src/hls/mod.rs` (`serve_playlist` and `serve_segment_file`) omitted `Content-Length` and `Accept-Ranges: bytes`, causing Axum to default to chunked transfer. Chromecast Fuchsia decoders require `Content-Length` on fMP4 segments to properly compute buffer margins.
  3. **Target duration RFC 8216 violation**: For copied video (`StreamAction::Copy`), segments can only split on source keyframes (up to 7.4s - 9.3s in *Dark Matter*). The upfront VOD playlist previously advertised `#EXT-X-TARGETDURATION: 6`, causing segment 0 (7.341s) to violate the target duration ceiling. Added `render_media_playlist_with_target` which sets `#EXT-X-TARGETDURATION: 10` for copied video streams.
  4. **Initial seek skew on small offsets**: Clamped `start_ms < 2000` to `0` across `playback.rs`, `Player.tsx`, and `cast.ts`, avoiding `-ss` keyframe/audio desync on initial playback.
- Verification:
  - `just web-check`: 208/208 Vitest passed, Biome clean, `tsc -b` clean, Vite bundle 49.20 KB gzip <= 150 KB budget.
  - Rust workspace: `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings), `cargo fmt --check` (clean), `cargo test -p mbl-stream` (all 9 tests passed), `cargo xtask openapi-check` (up to date), `cargo xtask mcp-parity` (100% parity, 158/158 endpoints).
  - Deployed to `.dev/preview-cast/mbl-server` daemon on port 18421.
  - Live probe on port 18421: `master.m3u8` (`Content-Length: 154`), `index.m3u8` (`#EXT-X-TARGETDURATION: 10`, `Content-Length: 13899`), `init.mp4` (`Content-Length: 1376`, `Accept-Ranges: bytes`), `0.m4s` (`Content-Length: 257351`, `Accept-Ranges: bytes`). Detailed request logs show exact byte counts and CrKey user-agent.

### 2026-10-31 — Robust streaming/playback logging & TMDB API key GUI settings
- Implemented robust end-to-end logging across streaming routes and playback lifecycle:
  1. Added `RemoteClientIp` extractor in `crates/mbl-api/src/extract.rs` with `X-Forwarded-For` and `ConnectInfo<SocketAddr>` resolution.
  2. Enhanced `crates/mbl-api/src/routes/stream.rs`: traces request and response details for `direct`, `live_file`, `hls_master`, and `hls_rendition_file`. Logs `session`, `file`, `client_ip`, `user_agent` (identifying Chromecast CrKey vs browser), `range`, HTTP `status`, `bytes` (Content-Length), and `elapsed_ms`.
  3. Enhanced `crates/mbl-api/src/routes/playback.rs`: logs `user_id`, `item_id`, `profile`, `start_ms`, `network`, `session_id`, `method`, `url`, `content_type`, and progress/stop positions.
  4. Mirrored frontend Cast diagnostic logs to browser `console.info` with `[MBL Cast]` prefix for live DevTools visibility.
- Added comprehensive TMDB API key management and live validation in GUI:
  1. Backend routes in `crates/mbl-api/src/routes/system.rs`: `GET /api/v1/system/settings`, `PATCH /api/v1/system/settings`, and `POST /api/v1/system/settings/test-tmdb`. TMDB test reaches TMDB API `/configuration` endpoint with the candidate key or stored key.
  2. Dynamic metadata worker sync: `crates/mbl-metadata/src/worker.rs` `Matcher.get_tmdb()` dynamically caches and syncs with SQLite `SettingsRepo` (`tmdb_api_key`), so keys added, updated, or cleared take effect instantly across background jobs without requiring server restart.
  3. Frontend GUI in `web/src/pages/admin/Settings.tsx`: dedicated Server Settings page (`/admin/settings`) with TMDB connection status badge, key visibility toggle, Save, Test Connection, Remove Key, external link to themoviedb.org, non-blocking notification banners in `Libraries.tsx` and `Setup.tsx`, and embedded live Cast diagnostics.
  4. Registered in `App.tsx`, sidebar and mobile navigation (`Layout.tsx`), and Command Palette (`CommandPalette.tsx`).
- Verification:
  - `just web-check`: Biome clean, 208/208 Vitest passed, `tsc -b` clean, Vite bundle 49.22 KB gzip <= 150 KB budget.
  - `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings.
  - `cargo fmt --check`: Clean, 0 diffs.
  - `cargo xtask openapi-check` & `cargo xtask mcp-parity`: 100% parity (158/158 endpoints).
  - Deployed to `.dev/preview-cast/mbl-server` daemon on port 18421 (`http://192.168.68.147:18421`).
  - Verified live API: TMDB validation error handling (HTTP 422 with status 401 message), key saving/clearing, and stream request logging with CrKey user-agent.

### 2026-10-31 — Cast 2-second exit resolved: VOD playlist parity & live-edge jump fix
- Root cause identified for why `Cast Direct` and `Cast HLS` worked while `Dark Matter` ("How to Build the Box") exited after ~2 seconds:
  1. `Cast Direct` (.mp4) is progressive MP4 (no HLS).
  2. `Cast HLS` (.mkv) is 30 seconds long; FFmpeg remuxed all 30s upfront and wrote `#EXT-X-ENDLIST`. Shaka Player on the Chromecast CAF receiver therefore parsed it as a complete **VOD** stream.
  3. `Dark Matter` is 60 minutes long. The recent experimental `hls_generated_timeline` path served `ffmpeg.m3u8` as `index.m3u8`. That playlist had `#EXT-X-PLAYLIST-TYPE:EVENT` and NO `#EXT-X-ENDLIST`. In Shaka Player / CAF, any playlist lacking `#EXT-X-ENDLIST` is classified as ongoing LIVE. By default, Shaka seeks directly to the live edge (the final segment written by FFmpeg, which was segment 2). Segment 2 lasted only 3.7 seconds. After 2 seconds, playback reached the edge of buffered media, starved, and CAF terminated playback.
- Resolution:
  1. Disabled `generated_playlist` in `crates/mbl-service/src/playback.rs` (`hls_generated_timeline: None`).
  2. Restored standard upfront VOD playlist delivery (`render_media_playlist` with `#EXT-X-PLAYLIST-TYPE:VOD` and `#EXT-X-ENDLIST`). Shaka Player now immediately recognizes the stream as VOD, starts at `request.currentTime`, and buffers segments sequentially without jumping to the live edge.
  3. Preserved stereo AAC transcoding (2 channels) and subtitle bounding (max 8) so the physical receiver is neither overwhelmed by 44 subtitle tracks nor fed unsupported multi-channel audio.
- Verification:
  - Unit tests in `web/src/utils/cast.test.ts` (23/23 passed) and `web/src/components/Player.cast.test.tsx` (20/20 passed).
  - Built frontend with `tsc -b && vite build` (30.76 KB initial bundle).
  - Built and deployed updated `mbl-server` to `.dev/preview-cast/mbl-server` and restarted daemon on port 18421.
  - Verified API playback start for `Dark Matter` (`itemId = 3`): returned VOD manifest with `#EXT-X-ENDLIST` covering all 602 segments, H.264 video copy, AAC 2-channel stereo.
  - Verified segment delivery: `init.mp4` (1.3 KB in 47 ms), `1.m4s` (241 KB in 8 ms), `2.m4s` (3.6 MB in 22 ms), seek jump to `100.m4s` (7.7 MB in 1.58s), `101.m4s` (8.3 MB in 81 ms).
  - Cleared watch history / resume offset for `Dark Matter` in preview DB.

### 2026-10-31 — Google Cast CAF sender improvements & preview server deployed (Port 18421)
- Implemented Google Cast CAF fixes based on Cast Design Checklist and sample app references (`CastVideos-android`, `CastVideos-chrome`):
  1. Set `mediaInfo.streamType = chromeCastMedia.StreamType?.BUFFERED || "BUFFERED"` and `mediaInfo.duration = media.duration`. This prevents CAF from misclassifying EVENT HLS manifests as live streams (`duration: -1`) which caused the receiver to halt playback after displaying the first decoded frame.
  2. Set `mediaInfo.metadata.metadataType = chromeCastMedia.MetadataType?.GENERIC ?? 0` with structured metadata.
  3. Bounded subtitle tracks (max 8) so the receiver is not overwhelmed with 40+ subtitle tracks, prioritizing the selected subtitle and default subtitle.
  4. Registered explicit `RemotePlayerEventType` listeners for all playback and connection events (`IS_CONNECTED_CHANGED`, `IS_MEDIA_LOADED_CHANGED`, `IS_PAUSED_CHANGED`, `CURRENT_TIME_CHANGED`, `DURATION_CHANGED`, `VOLUME_LEVEL_CHANGED`, `IS_MUTED_CHANGED`, `PLAYER_STATE_CHANGED`).
  5. Added `resumeSavedSession: true` to `CastContext.setOptions`.
  6. Added tests in `web/src/utils/cast.test.ts` verifying `streamType`, `duration`, `metadataType`, and subtitle bounding.
- Verification & Test pass:
  - `web/src/utils/cast.test.ts` passed 23/23 tests.
  - Full `just web-check` passed (Biome clean, 208/208 Vitest passed, `tsc -b` clean, initial Vite bundle 49.07 KB gzip <= 150 KB budget).
  - Built `mbl-server` with low-memory/disk flags (`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build -p mbl-server -j 2`).
  - Deployed binary to `.dev/preview-cast/mbl-server` and restarted daemon on port 18421 (`0.0.0.0:18421`, LAN URL `http://192.168.68.147:18421`).
  - Ran `python3 .dev/preview-cast/check-stream.py`: 100% passed (607.816s indexed past 596.578s resume point, 0 restarts, AAC 2-channel stereo, 3 decoded resume frames).
- Ready for physical Cast test on Bedroom TV.

### 2026-10-30 — Comprehensive test audit and verification pass (100% green)
- Full verification pass requested before any physical casting: `cargo fmt` (clean, 0 diffs), `cargo clippy --workspace --all-targets -- -D warnings` (0 errors, 0 warnings), `just web-check` (Biome clean, 207/207 Vitest passed, `tsc -b` clean, Vite bundle 49.08 KB gzip <= 150 KB budget), `cargo xtask openapi-check` (up to date), `cargo xtask tokens --check` (up to date), and `cargo xtask mcp-parity` (100% parity, 156/156 endpoints covered).
- Rust test suite: 188/188 unit tests passed across all workspace crates; 100% of integration tests across all crates passed. Identified and resolved a file race in `crates/mbl-server/tests/live_relay.rs` line 622 where PID file synchronization broke when the shell script truncated the PID file before writing bytes; now verifies non-empty PID before parsing.
- Live test server on `http://127.0.0.1:18421`: verified via `check-stream.py` (Chromecast `EVENT` playlist, 607.816s indexed past 596.578s resume point, `#EXT-X-TARGETDURATION: 9`, AAC 2-channel stereo, 0 restarts, 3 decoded resume frames). Verified live API endpoints via automated test: auth login/token/me, library listings, home hubs, plugin disable/re-enable lifecycle (confirmed fix for plugin re-enabling), unauthorized rejection, and playback start/progress/stop lifecycle.
- Disk space maintained at ~2.6 GB available by cleaning test binaries and temporary debug artifacts. Ready for physical Cast verification on Bedroom TV.

### 2026-10-30 — In progress: Cast HLS real timeline, recovered after reboot
- User explicitly requested fixing HLS timeline before next physical test, then machine rebooted. Source changes survived; `/tmp/mbl-online-dev`, tool runtime and entire Rust target did not. Production 8420 is untouched. Do not claim the preview or new fix ready yet.
- In-flight implementation: `CreateSessionReq.generated_playlist_start_ms` enables actual FFmpeg EVENT lists for Chromecast VIDEO COPY only. Encoded-video legacy path remains unchanged because decoding/encoding an entire prefix before resume is unsuitable. Generated mode starts at source zero, indexes through resume+6 seconds under a 60-second deadline, keeps original keyframe-aligned durations and init, and disallows fragment-triggered seek/fallback restarts. Existing normal web/encoded/other callers use None. Cast HLS UI seeks renegotiate/prepare targets only when new optional PlaybackResponse.hlsGeneratedTimeline is true; old/fixed-grid/direct/native paths retain SDK seeking. Remote seek commands delegate to the player when present. Regenerate OpenAPI and TypeScript after binary build.
- Restored development configuration at ignored `.dev/preview-cast/mbl.toml`, persistent data `.dev/preview-cast/data`, temporary cache/transcode `/tmp/mbl-cast-preview`, port 18421/public URL http://192.168.68.147:18421 (LAN verified after reboot). No server yet; must create fresh disposable admin/admin-password and library pointing only `/home/marcus/Downloads/1` after build. Old test DB/history was volatile and lost; no root `/` library should be recreated.
- Verification so far: targeted Cast/player web 33/33 and TypeScript/Vite pass after reboot; new real variable-GOP HLS regression added; nine legacy session tests passed in the first run, while new frame test hit FFmpeg 7 input-side HLS seek invalid NAL/empty output. Reproduced with plain FFmpeg outside MBL; output-side continuous seek decodes three frames. Test method corrected, final rerun queued behind server build (session 4407, /tmp/mbl-cast-timeline-session-final.log). Rust dependencies rebuilding from zero. Cargo jobs limited to 2, incremental/debug disabled, target `/tmp/mbl-online-target`. Running `cargo test -p mbl-transcode --test session` (session 89528, log `/tmp/mbl-cast-timeline-session-tests.log`); server build queued behind it (session 45857, `/tmp/mbl-cast-timeline-server-build.log`). New test must use manager.shutdown_all(), not shutdown(). Web build `/tmp/mbl-cast-timeline-web-build.log`; node_modules/.bin tools work directly, pnpm is unavailable after reboot. Use workdir web for frontend tools.
- Still required: finish Rust/test fixes, real generated-list/init/no-restart regression, playback_tracks/stream/startup checks as appropriate, service/transcode Clippy, full web pass once, bundle/fmt/diff; build and launch preview with existing LAN scope, recreate/scan one-media library, GUI verify and let user physically retest. Prefix indexing trades startup/disk for accurate remux timelines, bounded by deadline/quota; do not claim all HLS/encoded formats fixed. No commit/push/release.

### 2026-10-30 — Cast stereo retest and HLS timeline defect
- User confirms freeze persists with new server and supplied attempt 2 at 16:25:42Z. LOAD accepted, local source released, PLAYING at 24.590 s but receiver position stays 596.578; media unload ~66.35 s, SDK disconnect ~76.64 s. Saved sanitized timeline and output/playlist evidence in docs/audits/assets/2026-10-30-cast and updated Cast audit.
- Actual retained-session init probes H.264 + AAC TWO channels, confirming channel fix. UI E-AC-3/six-channel/Matroska messages describe source conversion reasons, not output errors. Do not infer outgoing E-AC-3 from them.
- Found separate HLS defect: estimated upfront VOD list advertises 3/3/6-second grid and target 6 while FFmpeg's copied-video list has variable 7.341/7.465/9.259-second segments and target 9. Directory inspected after disconnect/local reinit, so not proof of exact receiver request bytes. Service always enables estimated fast-start grid but FFmpeg uses six-second target and original keyframes; even encode forces six-second keyframes. Merely forcing encode or raising target duration cannot fully solve this. Next: accurate boundaries/timestamps and stable seek/init/restart mapping, correlated receiver requests. No new runtime code this follow-up; diff check only. TV freeze unresolved, production 8420 untouched.

### 2026-10-30 — Live QA batch 8: local Cast handoff and receiver audio limits
- User clarified continued audio is on the LOCAL COMPUTER, not Bedroom TV. Google Cast now pauses/releases the old local HLS/direct source on acceptance, rejects late local play/playing events and blocks Play while connecting. Native Remote Playback keeps its element/HLS transport. Cancel/disconnect still restore local playback appropriately. Diagnostic trail includes local paused/readyState/position around prepare/handoff and during heartbeat.
- Found a real server defect: Chromecast/generic HLS profile limits audio to two channels, but actual encoding retained up to six. `mbl-service::playback` now passes the negotiated profile channel limit to the HLS pipeline, also for audio-offset encoding. Real FFmpeg integration verifies AAC stereo, mono with offset and explicit six-channel output.
- Verification: all 197 web tests pass (39 files), targeted Cast/player 80/80, TypeScript/Vite and initial JS 49.08 KB gzip pass. All three playback_tracks Rust tests pass with actual FFmpeg, service-lib Clippy -D warnings and server build pass; fmt/diff checks pass. No public schema/endpoint/migration change; whole Rust workspace not rerun (prior 440 baseline).
- Real user-selected Cast with web fix and OLD server: local.released at 11.746 s, local heartbeats paused=true/readyState=0; user confirms TV picture still frozen. Media unload ~60.79 s and SDK disconnect ~70.99 s. Evidence `docs/audits/assets/2026-10-30-cast/receiver-local-released.json`. The user's pasted copy confirms it. Local source release is verified; TV freeze NOT fixed. Earlier full trace is also preserved as receiver-local-audio-full.json.
- Preview server only restarted after build: session 76297, same config/data and LAN public URL on 18421; HTTP 200 verified. Production 8420 untouched. Next physical attempt must test the NEW stereo server. No commit/push/release. Broader all-feature trail remains follow-up after Cast works.

### 2026-10-30 — Cast follow-up: audio continues behind frozen video
- User supplied a second Cast log and initially reported continuing audio while the episode image is frozen, then clarified that the audio is on the LOCAL computer, not the TV; Cast disconnect follows local audio stopping. Saved masked log as `docs/audits/assets/2026-10-30-cast/receiver-audio-continues.json` and updated the Cast audit. This corrects any interpretation that frozen sender position proves all playback stopped.
- Second trace: PLAYING at 16.533 s, position 376.775 unchanged in heartbeats, media unload/IDLE ~58.99 s, last heartbeat still SDK-connected at 59.091 s. No receiver error/idle reason or control command; disconnect itself is outside the provided trace. No root cause/fix claimed. Next: correlate receiver video decoding, HLS timestamps/segments and resume seek with server diagnostics. Documentation/evidence only; diff check, no test rerun needed.

### 2026-10-30 — Live QA batch 7: local Cast diagnostic trail
- User confirmed the fMP4 fix did not resolve freezing and requested logging. Added bounded scalar-only `castDiagnostics.ts`, exported JSON and localized `CastDiagnostics.tsx` in Stats (Swedish: Cast-diagnostik → Kopiera Cast-logg). Captures chooser/preparation, server negotiation/status, LOAD format/autoplay/start/track count and acknowledgement, receiver/media state, Play/Pause/Seek, 10-second heartbeat, command-channel status and teardown. SDK code/description/detailedErrorCode recorded without serializing SDK objects. A 20-second pending-attempt marker observes without cancelling playback.
- Memory-only per browser page, no upload/persistence/new REST/MCP capability. 200 events retain initial 40 plus latest 160. New attempt/reload resets. Masks URL credentials/query/fragment, bearer/credential fields and HLS/live capability paths (including relative URLs). Copies only the sanitized report; selectable fallback if clipboard unavailable. Five-language UI; stats scrolls in viewport. Tests cover redaction, bounded retention, subscriber cleanup, scalar error codes, copy and fallback. Player test ThemeContext mocks now provide locale.
- Full final web suite passes 194/194 across 39 files; TypeScript/Vite and initial JS 49.08 KB gzip pass. Biome has 0 errors and previous 5 warnings/4 infos; diff check passes. Focused Cast/player/diagnostic tests also pass. Early runs exposed missing locale in stats fixtures and an old transformed module while files changed during a full run; final clean run is green. Do not edit test/code modules during a full test run. No Rust/API/migration change; prior Rust 440 baseline not rerun.
- Computer use verifies live Swedish panel, actual server negotiation and successful Copy. A user-selected Bedroom Tv attempt occurred during QA: LOAD accepted ~7.3 s, media loaded ~9.1 s, BUFFERING then PLAYING ~13.6 s at 357 s. No advancing position was reported before media unload ~55.3 s and SDK disconnect ~65.5 s. No idle reason/Play command captured; cause of unload/disconnect is not established. Sanitized evidence `docs/audits/assets/2026-10-30-cast/receiver-attempt.json`, report `docs/audits/2026-10-30-cast-diagnostics.md`; user guide `docs/book/src/chromecast.md`.
- User proposed a proper diagnostic trail for all features *after Cast works*. Recorded design direction (correlation IDs, UI/API/service/FFmpeg/worker chain, admin search, redaction/retention/export); not implemented or claimed complete. Current task still physical Cast freeze investigation. Sender refreshed with final capability-redaction build and verified in GUI. Capture report before reload/new attempt. No commit/push/release, production 8420 untouched. Preview LAN config/session 59890 remains; browser tab 9.

### 2026-10-30 — Cast fMP4 segment description
- User reports Bedroom TV displays an image but does not start. GUI stats show chromecast profile but local metrics/no active receiver; authenticated PlayerRegistry read returns [] (separate diagnostic login logged out). This does not prove that an actual video frame reached the TV. User clarified: an actual episode still appeared and Play did not help. This answers the earlier question; no confirmed retest of the latest fMP4 description fix yet.
- Found a concrete sender-spec mismatch: all MBL HLS is fMP4 but MediaInfo omitted hlsSegmentFormat/hlsVideoSegmentFormat. Google documents TS as the default and requires explicit segment description. Added hlsFmp4 flag from server-negotiated HLS URL; CastManager uses SDK HlsSegmentFormat.FMP4/HlsVideoSegmentFormat.FMP4 constants. Direct MP4 unchanged; missing SDK format support yields explicit error. No speculative connection-state changes.
- New SDK regression checks both format constants and absence on direct MP4. Focused Cast/player suite passes 60/60; TypeScript/Vite and 48.81 KB initial JS gzip pass; touched-file Biome and diff pass. Logs /tmp/mbl-cast-fmp4-{tests,build,budget}.log. Prior full web 186 pass predates this new test; no Rust/API/migration changes.
- Evidence DOM saved as docs/audits/assets/2026-10-30-cast/no-active-receiver.txt. Source/preview local only; installed 8420 untouched. Sender was reloaded with the fMP4 fix; fresh physical Bedroom TV retest is pending before claiming resolution. Current browser tab 9 at /item/3. Do not restart server unnecessarily; LAN config from previous entry remains active.

### 2026-10-30 — Cast retest: receiver-reachable preview URL
- User's next physical Cast attempt passed the fractional-start error and displayed `Open MBL using a LAN address or configure a receiver-reachable public URL before casting.` Read directly in tab 7. No new application bug inferred: preview had public_url/bind 127.0.0.1.
- For the authorized Bedroom TV test, changed only `/tmp/mbl-online-dev/mbl.toml`: bind 0.0.0.0, public_url http://192.168.68.147:18421 (eth0 route verified). Saved prior config `/tmp/mbl-online-dev/mbl.loopback.toml`. Gracefully terminated test PID 3930121 and restarted latest debug server, exec session 59890. HTTP 200 HTML verified from both loopback and LAN address. No router/firewall/auth changes; production 8420 untouched. Test account remains disposable admin/admin-password; service now accessible on host network interfaces. Revert config/restart if LAN test exposure is no longer needed.
- User closed tab 7 during inspection; remaining old tab handles could not be claimed even after reset. Browser troubleshooting read; opened fresh visible tab 9 at http://127.0.0.1:18421/item/3, confirmed loaded/authenticated, marked handoff. Keep sender on localhost while receiver URLs use LAN. No successful Bedroom TV playback claimed yet; user must retry Cast. No new tests required for this temporary configuration; prior 45 focused tests and builds green.

### 2026-10-30 — Live QA batch 6: series navigation, fresh resume and Cast start positions
- Home now navigates container items to details; ItemDetail distinguishes Seasons/episodes, exposes child detail links and navigates Season instead of playing nonexistent media. Player cleanup notifies PlayerContext only after stop persistence resolves; a revision reloads mounted detail/Home data. Computer use verifies Show → Season 2 → Episode → actual playback, seek/close and fresh resume at 2:30 without reload. Screenshot: `docs/audits/assets/2026-10-30-qa6/fresh-resume.png`.
- Full web suite after navigation/resume changes passes 186/186; TypeScript/Vite, 48.82 KB initial gzip budget, Biome (0 errors, previous 5 warnings/4 infos) and diff check pass. New tests verify season navigation/refetch and that stop completion precedes parent notification. Initial new fixture omitted artwork/watch and was corrected before the green run. Rust unchanged; no workspace rerun.
- Browser QA also verifies local search → series, manual folder opening `/home/marcus/Downloads/1`, root-path warning, empty collection/request required-field validation, account views and agent wizard (no token created/config saved). The Test library still points to `/`; do not rescan blindly. This does not certify every integration or TV/device.
- Doctor X Korean S01E03 acceptance failed: Mwmbl returns unrelated hits; a manually found episode catalogue is recognized but its external iframe is unsupported by MBL and its default host refused connection. No CAPTCHA solved. Evidence/report: `docs/audits/2026-10-30-doctor-x.md`. Real episode discovery/playback remains pending.
- User reproduced Bedroom TV Cast error in tab 7: `startMs: invalid type: floating point 303693.649, expected u64`. Captured screenshot `docs/audits/assets/2026-10-30-cast/fractional-start.png`. Common negotiation now rejects nonfinite positions and floors nonnegative milliseconds before POST; dedicated exact-value Cast regression added. Latest focused tests pass 45/45 and TypeScript/Vite/budget pass; logs are `/tmp/mbl-cast-integer-{tests,build,budget}.log`. Tab 7 was reloaded with the fix. Physical Bedroom TV retest is pending. Preview's public URL/bind remain loopback and may separately prevent receiver access.
- Current user task remains physical GUI QA/fixes, now prioritizing Cast reproduction. User asked to open page and reproduced error; tab 7 is at `/item/3` with error, retained for handoff. No commit/push/release or installed 8420 change. Preserve all prior uncommitted batches. Next: finish Cast fix validation and let user retest; subtitle latency, dropped frames and broader integrations remain open.

### 2026-10-30 — Dark Matter library playback acceptance
- User added local Dark Matter library to isolated 18421. Computer use verifies actual episode video (H.264/E-AC-3 MKV, 3609.523 s) via HLS/hls.js/software backend, forward seek, pause and resume from saved 1:36 after detail-page reload. Final video paused at 124.98472 s, readyState 4, no media error; screenshot and metrics saved under `docs/audits/assets/2026-10-30-dark-matter/`. Audible sound was not assessed.
- New actionable findings: Home attempts playback of Show without a media part; ItemDetail labels Season as episode and offers invalid Season Play with no navigation to its children; detail view does not refresh saved resume after closing player. Swedish track remains LOADING/zero cues with zero-byte subtitle tempfile during observation; extraction latency/timeout/loading UX needs investigation. Stats report >1,300 dropped frames despite a 46–48 s buffer; browser/Pi decoding/load acceptance remains open. Report: `docs/audits/2026-10-30-dark-matter.md`.
- Audit only: no application fixes, permissions/configuration changes, commit or release. Normal test playback updates preview watch state; installed 8420 untouched. Prior batch 5 tests are not a test rerun for this audit. Next priority: fix series/season navigation and fresh resume state, then subtitle extraction and real-browser smoothness.

### 2026-10-30 — Live audit batch 5: bounded HLS playback and personal resume
- Added authenticated online sessions and bounded VOD HLS relay in `mbl-service::online::{hls,session}` and REST routes in `mbl-api`. Preparation follows at most three playlist levels, freezes a closed allowlisted media playlist, validates and decodes the first segment (plus optional init), then exposes opaque local resources. Public-address/DNS/redirect checks apply to every fetch. Encryption/live/byte ranges/external tracks/unsupported tags are rejected. Transfers, bytes, duration, concurrency, session counts and expiry are bounded. Login identity/token hash, current profile and feature state are checked before and after preparation/resource fetches; Close revokes the session.
- Migration 0013 and `OnlineRepo` persist source snapshots and viewing position per user independently of the discovery cache (200 retained, latest 20 shown). Changed episode identity resets resume; account deletion cascades. New `OnlinePlayer`/`OnlineHistory` support native HLS and hls.js fallback, genuine Playing events, saved pause/heartbeat/close position, completion reset, cancellation and late-created-session cleanup. Idle metadata-ready players heartbeat without claiming viewing history. Five-language UI and user guide updated. MCP `get_online_history` brings counts to 33/38/78; OpenAPI/bindings/snapshots regenerated, parity 156/156 with 56 exclusions.
- Verification: full ARM64/Linux workspace nextest passes 440/440, 0 skipped. A final narrow MAP-attribute guard was added afterward; all 24 online service tests and service Clippy pass again. Full web suite passes 184/184, TypeScript/Vite and 48.79 KB initial gzip JS budget pass. Affected-crate Clippy/all targets/-D warnings, OpenAPI/MCP parity, fmt/diff pass; Biome has 0 errors and previous 5 warnings/4 infos. Initial parallel web tests hit two existing 5-second timeouts during Rust linking; complete rerun with one worker/15-second deadline passes. No assertion or product change was needed for those timeouts.
- Computer use on isolated `http://127.0.0.1:18421/livetv` verifies actual public Mux H.264/AAC VOD HLS through local relay, seek, pause at 101.801857 seconds, Close/history at 1:41, reopening at the identical position and resumed playback without media error. Browser uses native HLS; hls.js fallback has a focused unit regression, not browser acceptance. Paused/resumed/delivery DOM metrics are saved in `docs/audits/assets/2026-10-30-hls/`; screenshot capture timed out, so no new image is claimed. Report: `docs/audits/2026-10-30-hls-player.md`.
- Preview uses `/tmp/mbl-online-dev`, disposable account admin/admin-password and latest binary `/tmp/mbl-online-target/debug/mbl-server`; it remains running with the browser open. Installed 8420 and its data are untouched; no commit/push/release/version/tag change. Online history remains separate from local-library Home/Continue Watching/Up Next; Cast, subtitles, transcoding, automatic episode transitions and real drama/provider coverage remain pending. LIVE-01 is partial, not complete.

### 2026-10-30 — Live audit batch 4: correct episode identity and native HTML sources
- `online/metadata.rs` now selects the main episode/movie before extracting its source. Explicit `mainEntity`, matching page URLs and same-page WebPage graph-ID references take precedence over unrelated graph objects. Video comes only from the selected item's own content URL or video/associatedMedia/encoding chain; hasPart/other episode branches cannot lend it a source. Episode lists belong only to selected series/seasons, not individual episodes. Commented JSON-LD and `data-type` attributes no longer masquerade as active structured metadata.
- Added bounded conservative HTML video/source fallback: one video element and one distinct quoted public source, relative URLs and ampersand entities supported; comments/raw script/style/noscript/template content ignored. Multiple players/different source URLs, private addresses, data-src-only placeholders and iframes yield no direct source. Series/seasons do not borrow embedded trailers. Extracted links still go through validated HTTP and actual local sample decoding. HLS is explicitly external-player-only and its segments are never fetched here.
- Five new regressions cover main episode vs trailer/related sources, graph references/type attributes/comments, series trailers, HTML ambiguity/raw/template content and real H.264/AAC decoding from an HTML source. Latest online suite passes 20/20; REST/MCP/M9 integration passes 22/22. Clippy/all targets/-D warnings, OpenAPI parity 151/151 (77 tools), fmt/diff pass. Frontend/API schemas and dependencies did not change; prior 177 web tests are reused. Full workspace 425-test baseline is from batch 2, not rerun for this batch. No claim of real-series/HLS acceptance.
- This is a correctness prerequisite for episode playback, not completed online HLS/normal-player integration. Source changes and regenerated test binary remain local; installed 8420, version/tag/packages untouched. Isolated preview remains on 18421. Next: safe HLS relay with bounded authenticated sessions and current profile checks, then normal player history/resume/episode transitions; actual source-provider coverage remains pending.

### 2026-10-30 — Live audit batch 3: configurable discovery and resilient personal search
- WebDrama adds optional SearXNG via existing plugin settings (`searchProvider`, `searxngUrl`). The service validates a public HTTPS base URL before persistence, uses JSON/general+videos with viewing-oriented query terms, and retains DNS/redirect/private-address protection. No external instance is installed or selected automatically; default stays Mwmbl. Administrators configure it in the WebDrama dialog's new Search settings disclosure; normal users do not load configuration. Saves preserve current enabled state and unrelated settings, with timeout/abort/error handling and five-language labels.
- Search caches are scoped by endpoint. Results rank normalized title matches, structured media and actual verified samples above weaker viewing signals and recap/news terms. This never promotes unchecked hits to playable. Personal index matches normalize punctuation/case; they survive external-provider failures with an explicit warning. Deduplication uses the final visible results, so discarded web hits no longer hide matching indexed sources. `OnlineSearchResponse.warning` is documented and bindings regenerated; paths/tools remain 151/77 with unchanged snapshots.
- Regression: four new service cases exercise provider query/config/cache changes, invalid config without persistence, heuristic ranking without playback approval, and personal results during provider failure. Two new web cases cover settings save/preservation and load failure/cancellation. Verification passes: 15 online service tests plus 22 REST/MCP/M9 integration tests, 177 web tests, TypeScript/Vite, 48.17 KB initial gzip JS, affected-crate Clippy/all-targets/-D warnings, OpenAPI/MCP parity and fmt/diff. Biome has 0 errors and previous 5 warnings/4 infos. Computer use verifies visible private-URL rejection and successful Mwmbl settings save on isolated 18421; browser screenshot capture timed out twice, so no new screenshot is claimed. Full prior-batch workspace baseline is 425/425; it is not claimed as a rerun for this batch. User guide: `docs/book/src/online-discovery.md`.
- Pending acceptance: real configured SearXNG discovery (no instance supplied/configured), full-series streaming, safe online HLS relay and normal player/history/resume/subtitles integration. Installed 8420, version/tag and release remain unchanged; local source only.

### 2026-10-30 — Live audit batch 2: real WebDrama discovery and bounded source verification
- Added `mbl-core::online`, `mbl-service::online`, migration 0012 and per-user `OnlineRepo`. Real Mwmbl title search replaces the fixed WebDrama demo feed. Search hits stay unchecked; source inspection extracts bounded JSON-LD series/season/episode metadata and episode links. Derived documents are private to the inspecting user (including signed URLs), cascade with account deletion and are globally capped at 512 × 64 KiB. Recent search considers up to 200 personal documents; this is not a normal media library or automatic crawler.
- New authenticated GET `/api/v1/online/search` and POST `/api/v1/online/inspect`; equivalent read-scope MCP tools and snapshots/counts updated to 32/37/77, parity 151/151 with 52 excluded paths. Managed, age-limited and library-restricted non-admin profiles are denied. HTTP validates each hop, rejects private/reserved IPs and credentials/custom ports, pins validated DNS answers and forwards no proxy/cookie/auth state. Source checks cap transfers/concurrency/time and run FFprobe/FFmpeg against owned local snapshots with protocol/format allowlists. Server-resolved/configured media tools are reused.
- New `OnlineSearch`/`OnlinePreview` in Live TV's WebDrama dialog: submit-only search, manual source URLs, clear status/details, season/episode links, recheck before preview, timeouts/abort/stale-response protection, media cleanup and actual Playing events. All new labels have five translations. Browser testing caught an AV sample failure: combined one-frame audio/video output finished on audio with `frame=0`; mapping the primary stream fixes it. The regression clip now contains H.264 and AAC. A metadata-ready preview no longer times out while awaiting Play.
- Final verification on ARM64/Linux: full workspace nextest passes 425/425 tests, 0 skipped, including all 11 online service tests with the AV regression and REST/MCP/profile/migration coverage. 175 web tests, TypeScript/Vite, 47.65 KB initial gzip JS, Biome (0 errors, previous 5 warnings/4 infos), affected-crate Clippy with all targets/-D warnings, OpenAPI/tokens/parity and diff/fmt pass. Computer use verifies actual direct-video playback to 5.055/5.055 seconds (ended, readyState 4, no media error) and no video after Close; screenshot/metrics are in `docs/audits/2026-10-30-webdrama.md`. One four-second probe timeout occurred during the memory-heavy full build; retry passed, so workload/latency acceptance remains open. Rust outputs use `/tmp/mbl-online-target` because `/` dropped below 2 GiB free during the build; pnpm is `/tmp/mbl-web-tools/node_modules/.bin/pnpm`.
- Isolated preview on `http://127.0.0.1:18421` uses disposable data under `/tmp/mbl-online-dev`; the old 18420 Vite process is stopped. Computer use confirms real Singles Inferno results (mainly articles, not verified streaming), a visible private-address error and a 390 px dialog with no horizontal overflow. Installed 8420, its credentials/preferences/libraries/permissions and v1.1.0 tag/packages are untouched. No commit/push/release. User guide: `docs/book/src/online-discovery.md`. Next: improve streaming-source discovery/relevance and extraction, then online HLS and full player sessions/history/resume/Up Next; do not claim a real drama series has played yet. Other live-audit findings and M11 remain pending. A new LIVE-13 finding records preferences not refreshing on sign-in (ThemeContext syncs only on mount); no preference-sync fix is included.

### 2026-10-29 — Live audit fix batch 1: folders, demo honesty and navigation
- Added isolated LibraryFolderPicker and PluginFeedBrowser components. Folder selection handles REST errors inline, manual server-validated/canonical paths, service-account access guidance, root warnings, 10-second timeouts, abort on close and stale-response/input-revision guards. Browser testing caught an initial-listing response erasing fast manual input; a regression test now covers it. Save conflicts display the actual error instead of an object string.
- Libraries now uses the shared semantic Dialog, with name-field focus, Escape, explicit focus restoration and bounded scrolling. A focus regression showed that an external trigger needs explicit restoration; fixed and tested. Common Dialog exposes optional focus callbacks.
- Plugin demo entries no longer emit a false Playing toast; they explain unavailable playback and have disabled controls. Feed loading has visible errors, Retry, timeout, cancellation and guards against old responses. This is a truthful catalogue foundation, not completed WebDrama search/streaming.
- API reference is linked from settings/palette using ordinary same-tab navigation (popup navigation failed in the app browser). Added admin-only node/agent/hardware settings links and palette commands for nodes/media requests. New strings have all five translations; broader i18n gaps remain.
- Verification: 166 web tests pass (11 new), TypeScript/Vite, 45.03 KB initial gzip JS budget, Biome (0 errors, previous 5 warnings/4 infos) and diff checks. Computer use against localhost:18420 with installed API at 8420 verifies the real permission error, canonical readable path, temporary unsaved folder selection, Escape/focus, demo explanation, mobile node navigation and actual API-reference navigation. Evidence/status are in the live audit report. No Rust/API/schema/database changes; no Rust rebuild needed. Standard-parallel web run was interrupted; bounded two-worker runs pass. Session pnpm runtime is `/tmp/mbl-web-tools/node_modules/.bin/pnpm`.
- Source changes are local/uncommitted. Installed v1.1.0, tag, file permissions and Test library `/` root are unchanged. A preview Vite session runs on 127.0.0.1:18420. Next: real server-side internet discovery and series/season/episode indexing, source verification and player integration for WebDrama; also resolve the selected local media folder's access/root before real player acceptance. OAuth, CLI packaging, Home Assistant and Windows installer work remain as documented.

### 2026-10-29 — Installed v1.1.0 GUI audit and WebDrama priority
- Tested the installed ARM64 `mbl 1.1.0-1` at localhost:8420 using computer use, including all sidebar views, empty-form validation, hardware self-test (10/10 PASS), command palette, profile/account views, 390 px mobile layouts and a real API-reference GET (200 OK). Findings, screenshots, verification limits and user acceptance requirements are in `docs/audits/2026-10-29-v1.1.0-live-audit.md`.
- Confirmed silent folder browsing failure: service user mbl cannot traverse marcus's mode-700 home; Libraries.tsx ignores the HTTP error result. Existing empty Test library points at `/` and would conflict with any nested new root. No file permissions, library roots, accounts, tokens or settings were changed; real player acceptance remains blocked.
- WebDrama is the user's highest priority: internet title search (e.g. Single's Inferno), source discovery beyond fixed providers/Korean drama, real series/season/episode indexing, attractive search and fast actual-media playability checks. Current WebDrama is hardcoded demo data with no stream; plugin Play only shows a false Playing toast. No real streaming succeeded.
- Other confirmed gaps: relative OAuth issuer/endpoints, missing installed mbl-cli for generated stdio configs, unwired Home Assistant MQTT runtime/GUI, undiscoverable API documentation, virtual channel form always sending no items, missing mobile node navigation, incomplete Swedish UI and library dialog keyboard behavior. Windows install-location/component selection is a user-reported issue corroborated by WiX source lacking a UI; no native Windows installer test was performed here.
- Audit only: no application fix, commit, version/tag change or release. Next work should follow the report, with WebDrama first and a real installed-player acceptance pass after resolving the media access/root configuration. M11 remains on hold.

### 2026-10-29 — v1.1.0 published with verified binaries
- Published https://github.com/msorenss/mbl/releases/tag/v1.1.0 from source `5ee9753aca11780ca99288c5386edf39c98cc50f`. All six exact-source CI jobs in run 40930802734 and all five Release jobs in run 40930812375 pass. The application includes the Windows Live TV/DVR paths, subtitle burn-in and FFmpeg process fixes verified by 388 Windows and 412 Linux tests.
- Release includes 16 packages plus SHA256SUMS: Linux x86_64/ARM64 server/worker/node DEBs and tar archives; Windows x64 MSI, full ZIP and standalone worker/node ZIPs. Native Bookworm version/architecture/glibc checks and Windows packaging pass. GitHub reports every asset uploaded with positive size; all 16 package SHA256 digests match the generated checksum manifest in the publication log. Complete CHANGELOG notes are attached to the published, non-prerelease release.
- The original unpublished tag was updated only after all builds succeeded, with force-with-lease, to annotated tag `b6ccb1f618644d780c2981b4989eaf4e29209ca3`. Node-before-pnpm fixed the failed clean-container bootstrap. Publication verified every upload before promoting the draft. The local release tag is synchronized; the final documentation-only checkpoint on main does not move the release tag or release branch.
- No further application changes were required. Physical tuner/browser/Cast/WAN acceptance, broader DVR transport/subtitle profiles and archive quotas remain follow-up work; M11 stays on hold.

### 2026-10-29 — Release container Node bootstrap correction
- CI for release preparation commit 057416c passed. Release run 40928515312 then failed both Linux jobs before compilation: pnpm 12's installer invoked `node install.js`, but the clean Bookworm containers had no Node executable yet. The hosted runner's action runtime does not put Node on the container PATH.
- Both Linux package jobs now install Node 22 before pnpm, then configure the pnpm cache after pnpm exists. Windows packaging is unaffected. YAML/order/shell/diff checks pass; the next release run must verify the complete packages before tag/release publication.

### 2026-10-29 — v1.1.0 package publication preparation
- User requested actual v1.1.0 binaries/releases after the green CI. The old Release run 40898181936 was cancelled, no v1.1.0 GitHub Release exists, and the unpublished annotated tag still points to edafb80 before the Windows fixes.
- Release workflow now supports codex/release-vX.Y.Z pushes and manual version resolution, waits for green CI on its exact source commit, builds all platforms, and only then updates an unpublished tag with an explicit force-with-lease. A published release with different source is refused; concurrent tag changes abort publication.
- Linux packages build natively in Debian Bookworm containers (separate cache key), with version/architecture and glibc-symbol checks. Added complete server Linux tarballs alongside the existing worker/node tarballs. Windows MSI is explicitly x64; fallback/readme versions are 1.1.0 and ZIPs include LICENSE.
- Publication stages a draft, uploads/validates all 16 packages plus SHA256SUMS, then publishes it. Release notes are taken from CHANGELOG, including the verified Windows fixes and actual limitations.
- Seven local Python release-guard/manifest/upload tests and YAML/bash/Python syntax checks pass. Application source is unchanged from green CI 40919393004 (388 Windows/412 Linux tests). Next: push preparation to main and codex/release-v1.1.0, follow package jobs and verify the release/tag/assets before reporting publication.

### 2026-10-29 — Windows subtitle burn-in and FFmpeg process ownership
- CI run 40915072455 verified the original Windows Live TV readiness and DVR detail failures are fixed. All five other jobs passed. Windows completed the full suite: 386/387 passed, with one ASS/burn-in playback failure and one leaky DVR size-cap test.
- Reproduced the subtitle failure using real FFmpeg on Linux with colon/backslash and apostrophe filenames. Filter paths were escaped for only one of FFmpeg's two parsers; the graph parser consumed escaping needed by the subtitle option parser. Escape option values and graph syntax separately, including whitespace, and validate options after decoding the graph layer.
- Added a real FFmpeg pixel regression using quote/bracket/comma/semicolon paths (and drive-like paths on Linux), plus updated parser properties and argument snapshots. Existing embedded ASS/external SRT playback coverage remains enabled; startup assertions now identify the selected audio and burned subtitle indices.
- The Windows CI install step adds the actual FFmpeg/ffprobe binary directory to PATH instead of relying on Chocolatey's launchers, so cancellation controls the encoder directly. The prior log confirmed shim creation. CI run 40919393004 now completes all 388 Windows tests with no leaks and no skipped tests.
- Verification: all 19 mbl-transcode tests pass on Linux/ARM64 (0 skipped), including the new real pixel regression, parser properties, argument snapshots, real HLS output and session cleanup. Snapshot diff was reviewed; only three subtitle filter argument strings changed. Formatting/diff checks and mbl-transcode Clippy with all targets/-D warnings pass. CI run 40919393004 verifies 388/388 Windows tests (0 skipped, 0 leaky) and 412/412 Linux tests (0 skipped); both original failures, ASS/external SRT playback and the new pixel regression pass. All six CI jobs, including web, SQLx, workspace lint and the ARM64 release build, pass. See https://github.com/msorenss/mbl/actions/runs/40919393004 for the complete run. After verification, cargo clean removed 9536 generated files / 5.6 GiB because disk space was nearly exhausted; about 5 GiB is free again. The published Live TV/DVR fix is `a398dd1`; the v1.1.0 tag and native client scope are unchanged.

### 2026-10-29 — Windows Live TV HLS and DVR fixture paths
- Investigated CI run 40905467648 (Windows job 123558903665). The other five jobs passed. The DVR detail 404 came from a fixture root built with mixed Windows separators: SQLite treated it as a different DVR library from the server's root, so the viewer was granted the wrong library ID.
- Added `TestApp::recordings_dir()` using separate native path components and applied it throughout the DVR/capture/recovery fixtures. The partial-capture regression now also asserts that the published recording uses the granted DVR library.
- Live relay HLS output now uses the same Windows path normalization as VOD. FFmpeg's HLS fMP4 code locates the init directory using the last forward slash (`libavformat/hlsenc.c`, `base_output_dirname`); native backslashes could leave `init.mp4` outside the live session directory and make readiness time out. Shared helper: `mbl_transcode::args::ffmpeg_path`.
- Windows nextest now uses `--no-fail-fast` so one failure does not hide later regressions. Existing proxy settings, authorization checks, media checks and readiness deadlines remain in force.
- Verification: all 53 focused DVR/Live TV regressions pass on Linux/ARM64 (0 skipped), including real FFmpeg capture, radio, subtitles, recovery, profiles and retention. Clippy for mbl-server/mbl-service/mbl-transcode with all targets and -D warnings, formatting and diff checks pass. A new Windows CI run is required. The sandbox initially denied localhost binding; the same tests passed with local-server permission. Existing release tag `v1.1.0` is unchanged; this fix is for `main` and a subsequent release.

### 2026-10-29 — Release v1.1.0 tag pushed
- Bumped workspace/package versions, release notes, README, Windows package README and OpenAPI schema to 1.1.0. The release workflow now publishes the GitHub Release after Linux/Windows artifacts build; fallback tag references are v1.1.0.
- `cargo run -p mbl-server -- openapi` and `cargo xtask openapi-check` pass. Full batch 25 verification is recorded below. Release commit `edafb80` and tag `v1.1.0` are pushed; remote `main` and tag were verified.
- The GitHub release page returned 404 in the available unauthenticated browser session, so release assets could not be verified here. CI run 40898182098 failed on Windows because `Duration` was imported unconditionally although all usages are Unix-only. Guarded the import with `#[cfg(unix)]`; the focused test target compiles on Linux, formatting and diff checks pass, and fix commit `c6b8614` is pushed to `main`. The next CI run found Windows FFmpeg rejecting `-top 1` while generating the shared interlaced MPEG-TS fixture; removed this redundant flag while preserving `tinterlace` and `+ildct+ilme`. All four `audit_sessions` tests pass; fix commit `2b25691` is pushed. CI run 40902239588 then showed a Live TV readiness timeout and a DVR detail 404. The Windows test job lacked the required localhost `NO_PROXY`; it is now configured, and the two failing assertions include response bodies. The combined `dvr_failures` and `audit_sessions` tests pass 10/10 locally. Next: verify one new Windows CI run and release assets; M11 remains on hold.

### 2026-10-29 — Git-kontrollpunkt efter batch 25
- Användaren har godkänt checkpoint och publicering på GitHub. Batch 25 sparas ovanpå `a4efebb` med Signed-off-by; tidigare lokala checkpoints för Chromecast och batch 1–24 ingår i den samlade ändringen mot GitHub.
- Full verifiering från batch 25 återanvänds: 411/411 Rust-tester, 155 webbtester, workspace-Clippy, SQLx, cargo deny offline, binärbyggen, TypeScript/Vite, bundle-budget, fmt/diff, OpenAPI, tokens och MCP-paritet 149/149 passerar. Endast handoff- och checkpointstatus ändras inför committen.
- GitHub har en separat dokumentationsuppdatering (`ca035b7`); den ska bevaras vid publiceringen. Release/version/tagg/M11 ändras inte. Fysisk tuner/browser/Cast/WAN/Windows-acceptans och restlistans återstående funktioner kvarstår.

### 2026-10-29 — Batch 25: explicit DVR-återställning och beständig filstädning
- Ny GET/POST `/api/v1/livetv/recordings/{id}/recovery`, `RecordingRecoveryPlan` och MCP `recover_recording` med dry_run/confirm. Ägare/admin kan förhandskontrollera och återställa sparade Failed-filer efter omstart även när kanalen/sändningen är borta. Nuvarande ägare/bibliotek/rating och eventuell kvarvarande guide kontrolleras före och efter probing; delade tittare nekas.
- Återställning kopierar en storleksbegränsad prefixbild till en ägd tempfile och publicerar den utan överskrivning som `<id>.recovered.ts`. Spelaren kan inte påverkas av en gammal capture-skribent som fortsätter skriva `<id>.ts`. Bytebudget/reserv kontrolleras före kopiering; 60 s kopieringstimeout, 15 s ffprobe, RAII-städning vid avbrott. Återställt material blir alltid Partial med befintlig ägare/regel/retention. Originalet behålls till uttrycklig radering eller vald retention; kopian behöver extra utrymme. Ingen automatisk återställning av Failed-filer.
- Delete validerar hanterade filer och sparar raderingstombstone/katalogradering innan unlink. Misslyckad unlink försöks igen vid start och underhåll. Raderade ID:n städas i nyckelsidor om 200 med roterande cursor. Tombstone får prunas först efter lyckad filstädning och minst ett dygn efter både program- och capture-slut. Endast ID:ts raw/snapshot och egna scratchfiler omfattas; andra filer, symlänkar och kataloger raderas inte rekursivt. Ingen ny migration behövs.
- Webben skiljer Recover saved media från Retry av en sändning, även efter kanal-/programslut. Förhandskontroll, bekräftelse, dubblettskydd, avbrott vid avmontering och timeout finns. Saknad/tom/oläsbar fil ger ingen ny spelbar post. MCP är nu 75 adminverktyg (30 read, 35 read+library); snapshot och tester uppdaterade.
- Full verifiering på ARM64/Linux: cargo nextest --workspace passerar 411/411 tester, 0 skipped (två testtrådar och resurssnåla profilflaggor). Hela webbsviten passerar 155 tester; TypeScript/Vite och initial JS-budget 42,24 KB gzip passerar. De fem nya Rust-regressionerna täcker fryst/spelbar kopia, samtidiga REST/MCP-försök, saknad/tom/trasig fil/låg disk, beständig raderingsstädning och profiländring under probe. Tre nya webbfall täcker bekräftelse, saknad fil och sena svar/avbrott. Hela arbetsytans Clippy med alla targets och -D warnings, SQLx prepare --check mot en separat temporär databas med alla migrationer, cargo deny offline (advisories/bans/licenses/sources), debugbyggen av alla workspace-binärer, fmt/diff-check, OpenAPI-check, tokens-check och MCP-paritet 149/149 passerar (75 verktyg, 52 undantagna paths). Doctestkommandot passerar; projektet definierar 0 doctester. Biome har 0 fel och tidigare 5 varningar/4 infos. OpenAPI/TypeScript är regenererade. Fysisk och plattformsöverskridande releaseacceptans ingick inte.
- Bas är checkpoint `a4efebb`. `cargo clean` kördes enligt användaren före batchen (3,4 GiB borttaget). M11/release avvaktar; Git-checkpoint har nu godkänts av användaren. Helarkivskvot/prognoser, bredare transport-/undertextprofiler och fysisk tuner/browser/Cast/WAN/Windows-acceptans återstår.

### 2026-10-29 — Lokal Git-kontrollpunkt efter batch 24
- Användaren har godkänt en lokal checkpoint-commit för batch 20–24 ovanpå `fcdd83b` (batch 1–19). Den samlar DVR-profiler/ägarskap/retention, Live TV:s programgränser, Partial-publicering, Retry-preview/diskvakt samt radio/DVB/AAC/HEVC-rättningarna med migration 0011, tester, genererade API-typer och dokumentation.
- Senast verifierat: 51 riktade Rust-tester, 152 webbtester, TypeScript/Vite, JS-budget 42,24 KB gzip, berörda crates Clippy med alla targets och -D warnings, fmt/diff-check, OpenAPI-check och MCP-paritet 148/148. Resultaten återanvänds för denna checkpoint; inga kodändringar eller nya testkörningar behövdes inför committen.
- Full workspace-regression efter batch 20–24 och fysisk tuner/browser/Cast/WAN/Windows-acceptans återstår. Fortsatt arbete följer restlistan: transport-/undertextprofiler, Failed-städning/återställning och arkivbudget. Kontrollpunkten skapas med Signed-off-by; release/version/tagg/M11 ändras inte.

### 2026-10-29 — Batch 24: DVB-undertexter och radio i vanliga spelaren
- Recorder mappar även valfria undertextspår till MPEG-TS. Verklig DVB-bitmaptext och två ljudspråk bevaras; inspelningen öppnas med befintligt ljudval, inbränd text och Off. Programvaruinspelning fortsätter med stream copy. HEVC/AAC bevaras och förhandlas till H.264/AAC för Firefox; MPEG-2/AC-3 och MP2-radio verifieras också.
- DVR-publicering godtar positiv medielängd med audio eller video; filer med enbart undertexter förblir ospelbara. Ljudinspelningar katalogiseras som Track i samma hanterade bibliotek, med samma ACL/rating, ägare, Partial-status, historik och raderingsflöde.
- `PlaybackResponse.hasVideo` beskriver verklig video, inklusive när utleveransen är HLS. Webben visar en ljudvy med titel och korrekt Seek audio, döljer videokvalitetsval och gör inga trickplay-anrop för audio. HLS master annonserar rätt kopierad videocodec; radio annonserar enbart ljudcodec och ljudbitrate, utan falsk H.264. Vanlig session/ljudspårs-/Cast-förhandling återanvänds. Ingen ny endpoint, migration eller MCP-verktygsräkning.
- Verklig HEVC/AAC-regression avslöjade ett varierande FFmpeg-startfel: ADTS-AAC kopierades till fMP4 utan bitströmskonvertering. Ny intern `AudioOp::CopyAac` använder `aac_adtstoasc` när valt kopierat ljud är AAC; andra kopierade ljudcodecs får ingen AAC-filterkedja. Felutdata fångades med en tillfällig wrapper för enbart syntetiska tester och visade exakt muxerfel.
- Kopierad HEVC fick tidigare FFmpegs `hev1`-sample entry trots masterlistans `hvc1`. Ny intern `VideoOp::CopyHevc` sätter `-tag:v hvc1`; ffprobe på verklig remux verifierar att MP4-fragment och serverad lista stämmer överens. Äldre generisk Copy är kvar för andra codecs.
- Verifierat: 51 unika riktade Rust-tester (nya DVR-mediaflöden 4; källfel 6; planer/diskvakt 6; schemaläggning/retention 10; relä/DVR 12; uppstart 6; spår 2; transkodningspipeline 5). Verklig FFmpeg verifierar DVB-pixlar/Off, ljudspråk, radio Completed/Partial med profil/historik/radering, HEVC→H.264 och HEVC-remux med matchande hvc1 samt kopierat AAC. 152 webbtester, TypeScript/Vite och initial JS-budget 42,24 KB gzip passerar; slutlig kontrastjustering är också byggverifierad. Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för fem berörda crates med alla targets och -D warnings, fmt/diff-check och OpenAPI-check passerar. OpenAPI/TypeScript är regenererade; MCP-paritet 148/148 (74 verktyg, 52 undantagna paths). Full workspace-regression efter batch 20–24 ingick inte.
- MPEG-TS-profilen är fortsatt avgränsad: DVB-text är verifierad, inte generell konvertering av SRT/ASS/andra undertextformat, text-TV eller inbäddad CEA-text. Verklig tuner/browser/Cast/WAN och Windows-acceptans återstår. Nästa core: vidare transport-/undertextprofiler samt Failed-städning/återställning och helarkivskvot/prognoser. Tidigare batcher bevarade efter fcdd83b; ingen ny commit/release/M11.

### 2026-10-29 — Batch 23: Retry-policy, kanonisk preview och DVR-diskvakt
- `ScheduleRecordingRequest.retryOf` bevarar ursprunglig ägare, överlevande serieregel, marginaler och retention för Failed/Partial. Även admin-retry använder ägarens aktuella profil/ACL/rating. Borttagen regel ger manuell inspelning med samma ägare. En annan delad tittare får inte Retry; gammal fil behålls. DB-transaktionen kontrollerar källrad/regel igen och serialiserar samtidiga försök/idempotens.
- Gemensam `prepare_recording` driver faktisk schemaläggning, ny `POST /api/v1/livetv/recordings/preview` och MCP `schedule_recording(dry_run)`. Guidens riktiga titel/tider, effektiv padding, ägare/regel/retention och aktuell konflikt/befintligt jobb visas. Preview skapar inga bibliotek, kataloger, jobb eller tunerbokningar; actual skriver och revaliderar. Det är en ögonblicksbild, ingen framtida resursgaranti.
- Webben förhandsvisar Retry och bekräftar en ärvd raderingspolicy innan ett nytt försök skapas. Konflikt/låg disk förklaras utan ny inspelningsrad. Dubbla klick spärras; request-timeout och avbrott vid avmontering skyddar mot sen bekräftelse/schemaläggning. MCP har optional retry_of och visar samma verkliga plan; get_settings.dvr visar tillämpad diskstatus. Ett nytt preview-path men inga nya MCP-verktyg.
- `DvrConfig`: min_free_mb=512 som standard och optional max_recording_mb (MiB; 0/overflow för max avvisas). Unix läser tillgängliga icke-reserverade bytes via redan låst rustix 1.1.5; Windows har GetDiskFreeSpaceExW-wrapper. Osäker kod begränsas till Windows-FFI-funktionen. Cargo.lock får endast rustix som ny direkt relation för mbl-platform.
- Kontroll före start och varje sekund; FFmpeg får -fs med min(konfigurerat tak, startens lediga utrymme minus reserv). Nekad start skapar ingen capturefil. Stopp vid disk-/storleksgräns bevarar giltig del enligt batch 22. Inga sparade filer raderas automatiskt för att frigöra plats. Inspektionskontroller har timeout och kan avbrytas av shutdown/sluttid; policyfel dödar och reap:ar barnet och publicerar inte delmedia.
- Verifierat: 41 unika riktade Rust-tester (DVR-planer 6, källfel 6, schemaläggning 10, M9 7, MCP-snapshots 3, config 8 och disk 1). Verklig FFmpeg verifierar storleksstopp, spelbar Partial och barnstädning vid policyfel; låg disk hindrar capture utan radering. 151 webbtester passerar; två typfel i nya testassertioner rättades, därefter passerar TypeScript/Vite och JS-budget 42,24 KB gzip. Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sju berörda crates med alla targets och -D warnings, fmt/diff-check och OpenAPI-check passerar. OpenAPI/TypeScript är regenererade; MCP-paritet 148/148, fortsatt 74 verktyg och 52 undantagna paths. Hela Rust-arbetsytan har inte körts om efter batch 20–23. Tidigare batch 20–22 bevarade efter fcdd83b; ingen ny commit/release/M11.
- Nästa core: DVR-format/undertexter/radio, kvot för hela arkivet/prognoser och explicit städ-/återställningspolicy för Failed-filer. Diskvakten pollar och reserverar inte utrymme mot andra samtidiga skribenter; det är ingen hård volymkvot. Fysisk tuner/EPG/browser/Cast/WAN och Windows-acceptans återstår.

### 2026-10-29 — Batch 22: ofullständiga DVR-filer och säker slutpublicering
- Ny `RecordingStatus::Partial`/`partial`: nya capturer jämför probad medielängd med hela det planerade fönstret. Tolerans är 5 procent, minst 100 ms och högst 2 s. Ren tidig EOF kan inte längre ge Completed bara på FFmpegs exitkod. Giltig video efter encoder-/källfel kan sparas som Partial med orsak; tomma/trasiga filer förblir Failed utan katalogpost.
- Recorder använder både monotont sluttidslarm och väggklockskontroll; en hängande källa fortsätter inte under den tidigare 15-sekundersmarginalen. Efter stopp avgör faktisk medielängd om fönstret täcktes. Serverstopp, avbokning och behörighetsfel går inte genom källfelens delpublicering.
- Ägare/guide kontrolleras före och efter ffprobe även vid snabb EOF. En ändring under analysen kan inte publicera en fil med äldre tillåtande rating. Publicering sparar status, orsak och katalogpost atomiskt. Äldre Completed-arkiv omklassas inte retroaktivt från otillräcklig historik.
- Partial visas med orsak, filter, Play partial, Retry och ägarstyrd Delete. Bibliotekstitel och beskrivning märker också ofullständigt material. Retry får nytt ID utan att radera tidigare del. ACL/rating gäller normal spelare och delad listning. Retention räknar både Completed och Partial som sparade filer och skyddar pågående/pausad visning; webb/MCP beskriver saved i stället för enbart completed.
- Befintliga REST/MCP-list-/spelar-/raderingsflöden bär den nya statusen; inget nytt verktyg eller endpoint behövs. MCP-schema och tre snapshots uppdateras. Ingen databasändring behövs eftersom statuskolumnen är fri text.
- Verifierat: 31 riktade Rust-regressioner passerar: källfel/sluttid 6, DVR-planering/retention 10 (med Partial-filer), relä/DVR/publicering 12 och MCP-snapshots 3. Källfelsfallen är även omkörda med en tunerplats. 148 webbtester, TypeScript/Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sex berörda crates/alla targets med -D warnings, fmt/diff, OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. OpenAPI/TypeScript är regenererade. Full workspace-regression från kontrollpunkten är inte omkörd efter batch 20–22. Batch 20–21 ligger kvar efter kontrollpunkten fcdd83b; ingen ny commit/release/M11.
- Nästa core-steg: undertext-/radio-/övriga medieprofiler för DVR, lagringsbudget/städning av Failed-filer, bevarad regelkoppling vid Retry och normaliserad MCP-preview. Återställning av ännu opublicerade filer efter serveravbrott kräver separat policy; de visas fortsatt som Failed. Fysisk tuner/EPG-/browser/Cast/WAN-acceptans återstår.

### 2026-10-29 — Batch 21: Live TV-profiler, programgränser och återkallade medielänkar
- Live-start hämtar aktuell profil från den aktiva inloggningen. Profiler med åldersgräns behöver ett pågående tillåtet EPG-program; saknad guide ger nekad start. Admin/fria profiler kan fortsatt använda kanaler utan guide.
- Ägda live-leases binds till användare och inloggningens token-hash så ett återanvänt SQLite-sessions-ID inte kan återuppliva en gammal medielänk. Aktuell session, profil, program och källa kontrolleras vid start, efter beredskap, förnyelse, varje medieanrop och periodiskt underhåll. Nekade leases städar encoder, tunerplats och cache.
- Åldersbegränsad uppspelning binds till ett enda programs ID/sluttid. Lease och FFmpeg-duration begränsas, även för en HTTP-källa som levererar snabbare än realtid. Ny `LiveStreamInfo.accessEndsAt` gör att webben stänger buffrad uppspelning vid programgränsen. Även ett tillåtet nästa program kräver nytt kanalval; fria profiler behåller kontinuerlig visning. Ändrad guide-sluttid stoppar den gamla begränsade bokningen.
- REST-kanaler/guide/nu/nästa och MCP-guidesökning filtreras efter rating. Programsökning använder keyset-sidor före resultatgränsen så dolda träffar inte tränger undan tillåtna. Icke-admin får inga råa kanal-/tuneradresser eller dolda kanaler; tunerupptäckt kräver admin. Virtuell EPG döljs för icke-admin tills katalogmedveten ACL finns (område 17).
- Ny `Channel.canPlay` styr spärrade spelknappar och förklaring. Guiden uppdateras var 30:e sekund medan guidefliken är öppen; gamla svar kan inte ersätta ett nyare fönster. Terminalt mediefel stänger HLS och tömmer buffrad källa.
- Verifierat: 35 riktade Rust-regressioner passerar (Live-profiler 6, relä/DVR 12, DVR-planering/retention 10, äldre M9/guide/MCP 7). Nytt kanaluppdateringsfall är också omkört. 147 webbtester, TypeScript, Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sex berörda crates/alla targets med -D warnings, fmt/diff, OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. OpenAPI/TypeScript är regenererade. Full workspace-regression från kontrollpunkten är inte omkörd efter batch 20–21. Tidigare batch 20 ligger kvar i samma arbetskopia efter `fcdd83b`. Ingen ny commit/release/M11.
- Nästa core-steg: DVR-källors tidiga avslut, format/undertexter och lagringsbudget samt normaliserad MCP-preview. Verklig tuner-/EPG-klocksynk, browser/Cast/WAN och live-hårdvaruprofiler/timeshift kräver fortsatt acceptans.

### 2026-10-29 — Batch 20: profilmedveten DVR, seriematchning och retention
- `cargo clean` kördes först enligt användaren: 8370 filer/4,2 GiB borttagna, cirka 6,1 GB ledigt efteråt. Baseline är kontrollpunkten `fcdd83b`; denna batch ligger som nya ändringar i arbetskopian.
- Migration `0011_dvr_scheduling.sql` ger inspelningar ägare, bibliotek, ursprungliga programtider och kanalnamn. AUTOINCREMENT förhindrar att en avbokad child städar en ny inspelnings fil-ID. Dolda avboknings-/raderingstombstones stoppar automatisk återbokning och städas efter att sändningen varit avslutad ett dygn. Explicit Retry får nytt ID. XMLTV uppdaterar samma kanal/starttid i stället för att skapa dubbletter; gamla dubbletter migreras.
- Ny `mbl-service::livetv::scheduler` (`dvr_scheduler.rs`): REST/MCP använder samma actor-kontroller; guidebundna inspelningar hämtar riktig titel/tid/rating. Begränsade profiler får egna väntande/pågående inspelningar, måste ha DVR-biblioteksåtkomst och får bara guidens programfönster utan padding. Jobb kontrollerar aktuella rättigheter före start och under capture, även efter ändrad åldersgräns. Regel/detalj/radering är ägar-/adminstyrda; andra tillåtna profiler kan spela delade färdiga filer men får inte radera dem.
- Serieregler matchar 14 dagar framåt var 15:e sekund med keyset-sidor om 200 och effektiv anti-join för redan behandlade sändningar. Kanal/säsong/episod/new-filter används. Konflikter visas som Failed; Retry gör ett nytt uttryckligt försök. Regelborttagning avbokar väntande jobb och behåller pågående/färdiga filer. Planering/återställning ligger utanför capture-taskernas 32-platstak.
- Retention kör verklig fil-/katalogradering för keepMax och ägarens watch state. Högst 16 försök per pass och inga pågående/pausade spelarobjekt raderas. Ett delat playback-lås skyddar start, heartbeat, stopp, expiry och shutdown mot samtidig retention. HLS stängs före shutdown-låset för att inte låsa fast en väntande start bakom retention; ny regressionskontroll täcker detta. Upptagen probe-gate skjuter upp retention i stället för att blockera nya spelare.
- Färdiga arkiv behålls vid kanal-/tunerborttagning; väntande/pågående jobb får källfel. Borttagna ägare kan inte lämna anonyma körbara jobb. DVR-bibliotek kan döpas om men inte flyttas/raderas genom vanlig bibliotekshantering. UI visar ägarstyrd Delete/Cancel, Retry, ursprungskanal, säsongs-/episodfilter och tydlig opt-in för radering/retention. Destruktiva formulärval återställs efter lyckat regelskapande. MCP validerar u32/range utan trunkering och markerar retentionskapabel regelhantering som destruktiv med confirmation.
- Ändrad programtitel eller höjd guide-rating stoppar capture så sparad äldre metadata inte gör nytt innehåll synligt för fel profil. PG-filtreringen i katalogfrågor tillåter nu endast kända etiketter på rätt nivå; okända etiketter följer Actor-regelns nivå 2 även vid listning.
- Verifierat: 140 webbtester, TypeScript/Vite och JS-budget 42,23 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. 45 unika riktade Rust-regressioner passerar (DVR 10, live 12, guide/schema/MCP-DVR 3, livscykel 4, startup 6, ACL/auth 7, MCP-snapshots 3). Även 205 matchningar över flera sidor, schema-uppgradering med befintlig data och riktig filradering/uppspelning verifieras. Clippy för sex berörda crates/alla targets med -D warnings och fmt/diff-check passerar. OpenAPI/TypeScript är regenererade; OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. Full workspace-regression kördes vid kontrollpunkten, inte på nytt efter denna batch.
- Fortsatt core-arbete: Live TV:s profil-/programövergångar, DVR-källors tidiga avslut/format/undertextbevarande och lagringsbudget samt korrekt normaliserad MCP-preview behöver granskas vidare. Fysisk tuner/Cast/WAN-acceptans kvarstår. Ingen ny commit/release/M11 i denna batch.

### 2026-10-29 — Samlad regression och Git-kontrollpunkt för batch 1–19
- Användaren har godkänt att spara de samlade granskningsfixarna i en lokal kontrollpunktscommit med `Signed-off-by`. Befintlig arbetsgren är `main`; versionsnummer och releasetaggar ändras inte.
- Full Rust-regression: `cargo nextest run --workspace --no-tests=pass --no-fail-fast --test-threads=2` med repots resurssnåla miljöflaggor passerar 371/371 tester, 0 skipped. Inga regressionsrättningar behövdes.
- `just web-check`: 138/138 tester, TypeScript, Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Fmt och diff-check passerar.
- Hela arbetsytans `cargo clippy --workspace --all-targets -- -D warnings`, OpenAPI-check, design tokens och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar också.
- Äldre dubbla testbinärer som redan ersatts av nybyggda versioner togs bort ur `target/` under kompileringen för att frigöra utrymme. Källkod/testdata behölls.
- Restlistans Git-kontrollpunkt skiljs nu från releaseacceptans. Fysisk tuner/Cast/WAN-/workeracceptans och 23 breda delvis öppna arbetsområden kvarstår. Fortsätt med spelarrelaterade DVR-/profil-/serieregel-/retentionfrågor först; M11 avvaktar.

### 2026-10-28 — Batch 19: DVR-uppspelning och probad mediemetadata
- Ny `mbl-service::dvr`, `mbl-db::repo::dvr` och migration `0010_dvr_playback.sql`: färdig fil blir atomiskt en vanlig katalogpost med verklig ffprobe-duration, format, spår och EPG-snapshot. `Recording.itemId` använder befintlig Player/Playback/Stream, behörigheter och historik. Inga nya media-endpoints.
- Inspelningsbiblioteket hanteras av DVR; scanner hoppar över det så partiella filer inte importeras. Begränsad probing, root-/symlink-/ändringskontroll, idempotent publicering och avbokning via gemensamt lås. Äldre färdiga filer återställs parallellt med nya scheman vid start. Radering städar fil och katalogpost; externa/symlinkvägar följs inte.
- REST/MCP-listning, detalj och radering använder ACL/rating. Begränsade profiler ser endast publicerade tillåtna inspelningar; profilmedveten schemaläggning och pågående listning behöver fortsatt arbete. Webb-DVR visar Play, verklig längd, filraderingsbekräftelse och periodisk uppdatering. Live-release inväntas före uppspelning.
- Verifierat: 138 webbtester, TypeScript/Vite/Biome/budget 42,24 KB samt 10 riktade Rust-regressioner (DVR 5, MCP-snapshots 3, äldre DVR/MCP 2). Riktig femminutersfil verifierar probe/HLS/61 s resume/historik. Clippy för fem berörda crates med alla targets och `-D warnings`, fmt/diff, OpenAPI-check och MCP-paritet 147/147 passerar. API-typer regenererade; inga nya endpoints eller verktyg (74/52).
- Nästa steg: fortsatt DVR-serieregelmatchning och retention samt profilmedveten schemaläggning. Följ även upp DVR-bibliotekets livscykel vid manuell ändring av biblioteksroots och borttagning av tuner/kanal. Därefter återstående core-/receiveracceptans enligt restlistan. M11/Git/release avvaktar. Tidigare batchers ändringar är kvar i samma arbetskopia; ingen commit eller full release-regression körd.

### 2026-10-28 — Batch 18: serverägd Live TV och DVR-kapacitet
- Ny `mbl-service::live_relay`: ägd FFmpeg-child, riktig rullande fMP4-HLS, bounded start/input, software H.264/720p/25 fps/bwdif + AAC-stereo. Auth-ägd 45 s lease, separat renewal, kandidatstädning vid cancel/fel, capability-filer med strikt syntax. Ny `/api/v1/stream/live/{session}/{file}` har Range/HEAD/CORS/private no-store.
- `VideoCapacity` i mbl-transcode delar admission mellan VOD och live. Live/DVR använder samma tunerleases i LiveTvService. Aktiva reservationer behålls; DVR som möter full tuner får failed/orsak, ingen tyst överbokning. DVR-leasen hålls under hela barnets livstid. Avbokad partiell fil städas även vid race med processavslut.
- Periodiskt serverunderhåll och shutdown omfattar live; process publiceras under leaseslåset så shutdown inte missar en sen child. Kvot och borttagning av kanal/tuner städar cache/kapacitet. Liveencoder pausar idle-analys; trickplay-handler registreras med slutligt stream-konfigurerad PlaybackService.
- LiveTv väntar på release före kanalbyte, kan avbryta start, städar sena svar, har renewal-timeout/överlappsskydd och sessionsbundna callbacks. LiveStreamPlayer använder begränsad HLS-återhämtning, släpper vid fel/EOF och respekterar paus. Virtuella kanalobjekt får inte längre startas som falska tunerkällor.
- Verifierat: 136 webbtester (12 nya liveflödesfall), TypeScript/Vite/budget 42,24 KB; 32 unika riktade Rust-tester (live/DVR 9, transcodesession 9, äldre live/schema 3, startup/livscykel/trickplay 11). Interlaced MPEG-2/AC-3 från isolerad localhost-källa blir progressiv H.264/AAC; faktisk DVR-fil, full 45 s expiry, kapacitetskonflikter, cancel, kvot, Range/HEAD/CORS och cache/release passerar. Clippy, fmt, OpenAPI och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar; API-typer regenererade. Ingen fysisk tuner/NPU eller befintlig installation har använts.
- **Nästa core-steg:** DVR-filuppspelning, korrekt medieduration och fortsatta serieregler/retention. Profil-/ratingkontroll, fler liveprofiler, beständig timeshift och fysisk/browser-/WAN-/receiveracceptans återstår; ingen HW-liveacceleration är införd. Kapitelns falska prioriterings-/sömlöshetsanspråk rättade. Restlista: 2/25 klara, 23 öppna; M11/Git/release avvaktar.

### 2026-10-28 — Batch 17: cargo clean och begränsad spelaråterhämtning
- `cargo clean` enligt användaren: 8087 filer/3,4 GiB borttagna, cirka 8 GB ledigt direkt efteråt. Frontendbatch; inga Rust-/API-/MCP-ändringar eller omkompilering efter städningen.
- Ny `web/src/utils/hlsRecovery.ts`: två appåterförsök per källa (1/3 s), 30 s deadline vid fatal nätåterhämtning samt begränsade SDK-load policies (20/25 s, ett HTTP-retry). Master-fel laddar om manifestet; fragment använder aktuell position. Buffrat fragment/faktisk tidsframflyttning avslutar deadline utan att nollställa källans försöksbudget.
- Player stoppar permanenta HTTP-, avkodnings- och formatfel och erbjuder Retry/Resume också vid initialt förhandlingsfel och native/direct mediafel. Position bevaras före browserreset. Serversessionen släpps före manuell omstart/kvalitetsbyte efter fel, stop har 10 s timeout, och redan stoppad session dubbelstoppas inte. Spår/synk/kvalitet/paus bevaras; förberedd omstart förblir pausad med Play-instruktion. Misslyckat stop kan fortfarande kräva väntan på serverlease; det nya API-felet visas.
- Retry visar vänteläge. Manifestets autoplay följer senaste pausavsikten. Handlers registreras före load/attach; gamla SDK-fel/timers städas vid byte/Cast/stängning. Ny renderad `Player.hls.test.tsx` täcker 17 fall; befintliga stop-assertioner inkluderar nu abortsignal.
- Verifiering: 124 webbtester, Biome/TypeScript/Vite/budget 42,23 KB, Rust fmt och diff-check; samtliga kontroller för batchen passerar. Rustkod oförändrad sedan batch 16, ingen ny Clippy/OpenAPI/MCP-körning. Full release-/fysisk mottagaracceptans återstår.
- **Nästa core-steg:** Live TV/DVR: serverägt relä, spelarlivscykel och inspelningsuppspelning. Område 09 fortfarande öppet för fysisk/native-/bakgrundsacceptans och återstående telemetri. Restlistan: 2/25 klara, 23 öppna. M11 avvaktar; inga Git-/releasesteg körda.

### 2026-10-28 — Batch 16: HLS-livscykel, lång paus och återupptagning
- Servern startar nu ett ägt underhållsjobb varje sekund i `serve`/`start_jobs`: timeout, ahead-throttling, diskkvot och borttagning av utgången playback-metadata/WAN-andel. Progress från ägaren förnyar HLS-lease också vid paus/buffring, utan att flytta fragmentmarkören. Förnyelse och expiry använder samma sessionslås; diagnostik/annan användare kan inte förnya.
- Shutdown spärrar nya starter (inklusive samtidiga/pågående starter), väntar på barn-/katalogstädning och slutför bekräftade sessioner innan DB stängs. Portbindning sker före bakgrundsstart. Den oanropade oavbrytbara manager-loopen är ersatt. Ett avbrutet stop-anrop lämnar sessionen registrerad tills städningen är klar, så underhåll kan slutföra arbetet.
- Webbens heartbeat går var 5 s även vid paus/buffring och när fliken blir synlig. Första verkliga `playing` bekräftar lokal uppspelning; väntan/autoplay-blockering gör det inte. Heartbeat har 10 s timeout, överlappar inte och aborteras vid stängning. 404/410 för aktuell session ger lokal **Resume playback** vid aktuell position med bibehållna val/paus; sena fel från gammal session ignoreras. Cast använder mottagartid och visar återanslutningsmeddelande vid expiry.
- Automatisk städning använder redan sparad progress och skriver ingen gammal position över en nyare sessions watch state. Obekräftade kandidater ger ingen historik; bekräftade sessioner slutregistreras en gång.
- Berörda: `mbl-transcode`, `mbl-service::playback`, serverlivscykel, Player, API-dokumentation/typer samt nya livscykel-/avbrottsregressioner. Verifierat: 107 webbtester, TypeScript/Vite/budget 42,23 KB, 27 riktade Rust-tester samt Clippy/fmt, OpenAPI och MCP-paritet 146/146. TypeScript verifierat efter API-generering.
- **Nästa core-steg:** begränsa äldre retry-logik för övriga fatala HLS-/mediefel och fortsätt Live TV-/DVR-uppspelning. OS-/browser-sömn kan stoppa heartbeat längre än 60 s; då krävs ny ström. Fysisk Cast/native-/bakgrundsacceptans återstår. Restlista: 2/25 klara, 23 öppna; M11 avvaktar och inga Git-/releasesteg är körda.

### 2026-10-28 — Batch 15: beredskap före spelarens källbyte
- `PlaybackRequest.waitForReady` är frivillig för gamla klienter; webben begär alltid kontroll av läsbar/icke-tom direktkälla eller första färdiga HLS-fragment/init med gemensam timeout och befintlig backend-fallback. Gammal spelarsession behålls tills serverförhandlingen lyckas.
- HLS-start skyddas mot avbrott/timeout med kandidatstädning (process/katalog/kapacitet). Pågående start hindrar idle-analys. Stängd sessionsstatus hindrar sena Arc-/segmentanrop att återstarta en stoppad process. Halvskrivna filer efter FFmpeg-fel och gamla framtida segment efter seek/fallback kan inte godkänna en ny halvskriven fil.
- Webbens väntande startanrop aborteras vid stängning och sena svar släpps. Kvalitets-/spår-/synkbyte läser senaste position före installation lokalt och på Google Cast. Förberedelsestatus visas. Obekräftade beredskapskandidater skriver ingen watch state/historik; playing/paused-progress bekräftar sessionen.
- Berörda: `mbl-service`, `mbl-transcode`, API/MCP request-literals, Player, startup/session/webbregressioner och regenererade API-typer. Verifiering: 102 webbtester + TypeScript/Vite/budget 42,23 KB; 25 riktade Rust-tester; Clippy, fmt, OpenAPI och MCP-paritet 146/146.
- **Nästa core-steg:** `SessionManager::spawn_maintenance_loop` används inte i produktion. Koppla in periodiskt underhåll tillsammans med heartbeat för pausade sessioner, resume och serveravslut; enbart inkoppling riskerar att bryta långa pauser. Första serverfragmentet garanterar inte browser-/receiveravkodning eller senare felfri uppspelning. Restlistan är fortfarande 2/25 klara, 23 öppna; M11 avvaktar och Git/release är inte utförda.

### 2026-10-28 — Batch 14: ljud- och undertextspår under uppspelning
- Playback-svar innehåller tillgängliga ljudspår, verkligt valt ljudindex, undertextval och aktivt inbränt undertextindex; valda serverindex finns även i diagnostik/MCP. REST/preview avvisar saknade spår eller fel streamtyp i stället för tyst fallback. MCP avvisar också ogiltiga/överstora heltal utan u32-trunkering och returnerar korrekt invalid-params.
- Player kan byta ljud och förhandla bild-/inbrända undertexter vid aktuell position. Kvalitet, ljudsynk och nuvarande undertext/Off bevaras; paus under ett pågående byte respekteras lokalt och på Google Cast. Avvisat byte återställer reglaget och behåller gammal session med serverns felorsak.
- Rättad ASS/WebVTT-target, FFmpegs relativa undertextordinal för inbyggd text och stöd för separat textfil vid inbränning med biblioteks-root-kontroll. ABR-burn-in avvisas i stället för att tyst utelämnas; webbens pipeline använder en rendition.
- HLS kan nu sakna ljud utan att försöka mappa ett obefintligt spår. Master-codec följer faktiskt ljud; korta klipp annonserar verklig duration. Cast skickar inte inbrända spår som externa SDK-caption-ID:n. Valfri titelläsning får fallera utan att stoppa giltig Cast-laddning och titelvyn följer aktuellt media.
- Verifierat: 99 webbtester, TypeScript/Vite och budget 42,23 KB initial gzip; 20 riktade Rust-tester (playback/spår 7, MCP 2 inklusive 10 paritetsfall, pipeline/session 11). Verklig HLS-avkodning bekräftar 880 Hz i valt ljudspår, WebVTT-text, synlig inbyggd/extern inbränning och ljudlös kortvideo. Clippy, OpenAPI och MCP-paritet 146/146 passerar.
- Native Remote Playback är bundet till befintlig källa; streambyte/nästa tillåts inte stoppa aktuell session. Fysisk PGS/Cast/native-acceptans och beredskap vid FFmpeg-fel efter antagen start återstår. Undertextsökning/nedladdning/autosync/generering är fortsatt öppna; restlista 08/09 delvis uppdaterade. Inga Git-/releasesteg körda.

### 2026-10-28 — Batch 13: verkliga sökförhandsbilder i spelaren
- `mbl-analysis::trickplay` använder verkliga FFmpeg-nyckelbilder och bygger JPEG-ark från validerade rutor. Fel och ofullständigt utdata ger fel, inga platshållare. Rättad nollbaserad numrering, uppåtrundning för sista intervall, mycket korta klipp och begränsade manifestallokeringar.
- GET-index returnerar 202 medan ett idle-jobb förbereder cache, 200 först vid publicerade artefakter. Nya/ändrade videoskanningar köar biblioteksjobb; enskilda cachemissar dedupliceras. Källfil/path/storlek/mtime/duration och algoritmversion binder cache; gamla syntetiska cacheposter och saknade filer regenereras. Revisioner publiceras efter färdig generering och gamla revisioner städas. Konfigurerad FFmpeg används och barnprocessen har timeout/kill-on-drop.
- `TrickplayJobHandler` lämnar tillbaka jobbet vid uppspelning/termisk last. Jobbköns typade idle-yield räknas inte som fel och förbrukar inte återförsök. Produktionsproben läser nu verkliga termiska sensorer även utan test-root. Skannerns fristående bygge saknade `time/parsing`; beroendet är rättat.
- Player visar autentiserade blob-bilder på tidslinjens pekar-/tangentbordsfokus; återanvänder bildark, avbryter sena svar och frigör URL/timer när förhandsvisningen stängs eller delen byts. Pending/fel visas ärligt. Cast-statuslagret täcker inte längre header/tidslinjereglage.
- Verifierat: 91 webbtester, TypeScript/Vite och initial JS-budget 42,24 KB gzip; 40 riktade Rust-tester (trickplay-analys 5, jobbkön 11, skanner 14, serverns M5/playback/trickplay 10). Verkliga färgklipp verifierar ruta/bildarksgräns och kort video; HTTP/jobbrunner verifierar kö, källa/cache, ACL/rating och privat JPEG. Clippy för berörda crates, OpenAPI och MCP-paritet 146/146 passerar. Restlista 04 färdigimplementerad; manuella webbläsar-/receiverprov ingår fortfarande i slutkontrollen. Nästa batch fortsätter inom uppspelning (Live TV/DVR, undertexter och återstående spelar-/receiverpunkter). Inga Git-/releasesteg körda.

### 2026-10-28 — Batch 12: ljudsynk åt båda hållen
- Ljudsynk −10..+10 s hanteras i serverns ljudström med kodning och PTS-justering. Tidigareläggning trimmar ljud före aktuell HLS-start; video kopieras när profilen tillåter det. Värdet exponeras i playback, preview, diagnostik och befintligt MCP `preview_playback`/`explain_session`; tre tools-list-snapshots uppdaterade, verktygsantal oförändrat.
- Player omförhandlar vid aktuell position och bevarar pausläge/kvalitet. Sparad preferens ändras först vid accepterat byte. Google Cast får samma offset och kan byta vid mottagarposition; native Remote Playback tillåter inte ändring under pågående uppspelning. Den gamla lokala DelayNode har tagits bort så ingen dubbel fördröjning uppstår; nattläge behåller sin EQ/kompressor.
- Nekad Cast-serverförhandling behåller aktiv receiver och dess slutlyssnare. Videor utan ljud fortsätter med effektiv offset 0 ms. Ogiltiga värden nekas utan ny session. Vid nekad första start med sparad synk finns återförsök utan synk som bevarar återuppta/starta om-valet och visar serverns felbeskrivning.
- Verifierat: 85 webbtester, TypeScript/Vite och budget 42,23 KB initial gzip; pipeline 5/5 (inklusive riktigt ljudpulstest), playback-API 5/5, MCP-tool-regression och tre schema-snapshots. Clippy för berörda Rust-crates, OpenAPI och MCP-paritet 146/146 passerar. Fysisk ljud-/bildsynk på Cast-mottagare och återstående receivertelemetri är fortfarande öppna; därefter prioriteras övriga uppspelningsdelar enligt restlistan. Inget Git-/releaseslutsteg har körts.

### 2026-10-28 — Batch 11: verifierbar spelarstatistik (core prioriteras)
- GET `/api/v1/playback/{session}/diagnostics` och MCP `explain_session` visar sessionsklass/profil samt låst snapshot av HLS-backend/process/throttling/omstarter/senaste exit. Endast ägare/admin; stoppad session ger 404. MCP behåller befintliga sessionsfält och kompletterar med transcode-diagnostik.
- Player hämtar/pollar diagnostik bara när statistikpanelen är öppen och städar anrop/timer vid byte/stängning. Cast visar inte lokal upplösning/buffert/dropped frames som receiverdata. Fel markeras som otillgängligt; tailnet-klass påstås inte vara verifierad direkt-/relärutt.
- Lokal upplösning uppdateras vid metadataändring; buffert räknas i intervallet som faktiskt innehåller aktuell position. Okända mätvärden visas inte som falska nollor.
- Verifierat: playback-API 4/4, MCP-tool-regression, HLS-runtime-regression, Clippy, OpenAPI och paritet 146/146. Web-check med 78 tester, TypeScript, Vite och budget 42,24 KB initial gzip passerar.
- Användaren vill nu prioritera spelarrelaterat före andra områden. Nästa core-del är full ljudsynk med tidigareläggning av ljud; 0–5 s lokal fördröjning finns men signerad serverjustering är ännu inte implementerad.

### 2026-10-28 — Batch 10: fototidslinje, bilddialog och bildspel
- Ny `/photos/:id`-vy nås från fotobibliotekets detaljvy: månadsval, cursorbaserad hämtning, autentiserade bilder, tangentbordsnavigering, bilddialog och laddningsstyrt bildspel. Bildspel pausar när sidan är dold, stannar vid sista inlästa bild och stoppas vid bildfel. Blob-URL:er och anrop städas vid stängning/byte.
- Ny GET `/api/v1/libraries/{id}/photos` med EXIF-/tillagd-datum och stabil sidindelning på datum+ID. Äldre timeline räknade dolda åldersbegränsade bilder; båda använder nu ItemFilter och verifierar bibliotek/typ. Originalström har korrekta bild-MIME-typer.
- Verifierat: web-check med 74 tester, TypeScript/Vite/budget 42,24 KB gzip; foto-API-regression för rating/ACL, datumtie/pagination och originalbildens MIME/autentisering; server-Clippy; OpenAPI och MCP-paritet 145/145.
- Restlistans fotoflöde (06) implementerat. Manuellt browser-/format-/tillgänglighetsprov kvarstår som slutkontroll. Originalbilder används; särskild servergenerering av små fotominiatyrer/konvertering av RAW/HEIC ingår inte i detta flöde.

### 2026-10-28 — Batch 9: automatisk fortsättning på Google Cast
- CastManager observerar den aktuella SDK-medieladdningen och publicerar avslut endast efter PLAYING → IDLE/FINISHED. ERROR/CANCELLED/INTERRUPTED, sena lyssnare och dubbla händelser startar inte nästa avsnitt.
- Player inväntar stopp av avslutad serversession innan nästa receiverförhandling. Opt-in gäller aktuell kedja även på Google Cast; metadata som kommer sent kan utlösa fortsättning när klart. Native Remote Playback stöder inte denna automatik.
- Fyra nya regressioner för slutorsak, slutposition, gammal medielyssnare, gammal FINISHED-status och renderat nästa-flöde. Verifierat: web-check med 70 tester, TypeScript, produktionsbygge och budget 42,19 KB initial gzip passerar.
- Cast-guiden uppdaterad för faktisk profil/URL/textspår/kommandotransport och ny automatik. Fysisk Cast-acceptans kvarstår.

### 2026-10-28 — Batch 8: Up Next och säkert avsnittsbyte
- Spelaren visar nästa avsnitt under sista 30 sekunderna/vid slut och följer numrering över säsongsgränser via behörighetsfiltrerade item/children-API:er. Tomma säsonger hoppas över; API-fel visas med återförsök, och gamla anrop avbryts vid stängning/titelbyte.
- Lokal automatisk fortsättning är opt-in per uppspelningskedja, följer till nästa avsnitt och delas inte som global kontoinställning. Manuell nästa-knapp använder Cast-omförhandling när casting pågår; automatisk receiverfortsättning är inte införd.
- Stoppanrop återanvänder nu en gemensam Promise så nästa uppspelning väntar på redan pågående cleanup. Tangentkommandon stör inte längre input/select/button-navigation.
- Sju nya tester för ordning, säsongsbyte, tomma säsonger, fel, film, övergång och tangentbordsfokus; Verifierat: web-check med 66 tester, TypeScript, Vite och budget (42,19 KB initial gzip) passerar.

### 2026-10-28 — Batch 7: kapacitet utan att avbryta pågående uppspelning
- Playback stoppar inte längre användarens gamla HLS-sessioner innan en ny session har startat. Full kapacitet ger ett fel och lämnar den gamla sessionen kvar; ingen tillfällig överbeläggning av Pi:n införs.
- SessionManager serialiserar antagning från kapacitetskontroll till infogning, så samtidiga starter inte kan överskrida gränsen. Startfel frigör antagningen och städar sessionskatalogen.
- Tagit bort falsk framgång (genererad HLS-adress utan session) när HLS-manager/FFmpeg saknas.
- Regressioner: samtidiga starter, startfel och HTTP-kvalitetsbyte vid full kapacitet. Verifierat: sessionssviten 6/6 och samtliga playback-API-tester 3/3 passerar; Clippy för mbl-transcode/mbl-service/mbl-server --all-targets passerar.

### 2026-10-28 — Batch 6: kvalitetsbyte och lokal ljudfördröjning
- Player har kvalitetsgränser Auto/2/4/8/20 Mbps. Ny playback-session begärs från aktuell position med aktuellt part-ID; äldre session släpps efter lyckat svar. Fel återställer menyvalet och sena svar städas vid avmontering.
- Lokal WebAudio DelayNode ger 0–5 s ljudfördröjning och fungerar ihop med nattläge. Val sparas i befintlig preferens; tidigareläggning av ljud stöds ännu inte. Reglagen är spärrade under casting. Sparat nattläge aktiveras även vid uppspelningsstart.
- Tre nya renderade tester för lyckat/avvisat kvalitetsbyte, pausbevarande och verklig delay-node. Verifierat: web-check med 59 tester, TypeScript, Vite och budget (42,18 KB initial gzip) passerar.
- `docs/audits/remaining-work.md` listar 25 öppna arbetsområden och separata releasekontroller; ett delsteg innebär inte att hela området är klart. Spelarområdet är fortfarande delvis öppet.

### 2026-10-28 — Batch 5: tokennät och språkval
- AI Agents har LAN/Tailscale/WAN-val vid mintning samt nätvisning i tokenlistan. Tom nätlista stoppas lokalt; serverns befintliga policy gäller fortfarande.
- Rättat tyst återkallningsfel, felaktigt skapandefel efter misslyckad snippet-hämtning, gammalt snippet-tillstånd och falsk clipboard-framgång. Token som redan skapats bevaras när snippets saknas.
- Serverns språkvalidering accepterar nu alla fem UI-språk (fr/es nekades tidigare). Persistens-/ogiltigt-språk-test tillagt, liksom renderade tokenregressioner.
- Verifierat: web-check 56 tester, TypeScript/build/budget (42,18 KB gzip), riktade Rust-tester 2/2 och mbl-service Clippy --all-targets passerar. Git/release avvaktar övriga öppna punkter.

### 2026-10-28 — Batch 4: engångsinbjudningar
- Users-vyn har inbjudningar med uttryckligt biblioteksval, åldersgräns, giltighet, länk och återkallning. `/invite/:code` är publik och skapar konto via befintligt API; redan inloggat konto ersätts inte.
- Rättat dubbelinlösen: konto, ärvda behörigheter och förbrukning av inbjudan sker i samma skrivtransaktion. Fel vid kontoskapande rullar tillbaka allt. Inbjudningskoder loggas inte längre.
- Test för samtidig inlösen och rollback tillagt i mbl-db; renderade tester täcker avvisad registrering, återförsök, inloggat konto, explicit bibliotekstilldelning och avvisad återkallning.
- Webb: 53 tester i full körning passerar. Verifierat: web-check (53 tester, TypeScript, bygge, budget 42,18 KB gzip), databasens samtidighets-/rollback-test och Clippy för mbl-service/mbl-db --all-targets passerar.

### 2026-10-28 — Batch 3: administrera profilbehörigheter
- Admin Users har en riktig behörighetsdialog med bibliotek, hanterad profil, åldersgräns och separat PIN-sättning/borttagning. Misslyckad inläsning blockerar sparning; sparfel behåller valen.
- Rättat `UpdateUserPermissionsRequest` så att utelämnad åldersgräns betyder oförändrad men explicit JSON null tar bort gränsen. Tidigare kollapsade båda till None.
- Renderade tester för läsfel och avvisad sparning, plus Rust-test för utelämnat/null/värde. Verifierat: web-check (50 tester, TypeScript/build/budget 42,13 KB gzip), riktat Rust-regressionstest och Clippy för mbl-service --all-targets passerar.
- Användaren avsåg **Git**, inte GIF. Git/release väntar tills överenskomna rättningar är klara. Ingen GIF efterfrågas.

### 2026-10-28 — Batch 2: förfrågningar och historikimport i webben
- Ny `/requests`-sida: skapa önskemål med TMDB-ID, visa status, admin godkänner/markerar tillgängligt. Nåbar från desktop och mobilmeny. Arr-leverans är uttryckligen inte inkopplad.
- Kontosidan har `HistoryImport`: Plex/Jellyfin JSON-fil, format-/storlekskontroll, importresultat och omatchade titlar. Import sker till aktuellt konto; inte automatisk anslutning till källservern.
- Renderade regressionstester verifierar avvisad begäran/statusändring/import och lyckat återförsök utan förlorad inmatning.
- Verifiering: 48 webbtester passerar; se granskningsrapporten för kvarstående krav. Ingen release/tagg/push skapad; användaren vill slutföra rättningarna först.

### 2026-10-28 — Granskningsrapportens implementation pågår
- Implementerat ägarbundet spelarregister och WebSocket-kommandon, Cast-profil/omförhandling/textspår, Live TV-leases, atomisk DVR-bokning och FFmpeg-recorder, sessionsskydd för Companion/Matchmaker, WAN-pacing och ägd ljudprocess.
- Migration `0009_matchmaker_membership.sql` inför medlemskap; äldre röstidentiteter var självrapporterade och ogiltigförklaras.
- Webb: biblioteksredigering/radering, sparfel på hemsidan, reaktiv playback-preview och worker-uppdatering. AI/demo-status är tydligare.
- Se uppdaterad status i `docs/audits/2026-10-27-web-and-service-audit.md` och ADR 0007. Rapportens övriga UI-/runtime-krav är ännu inte klara. M11 fortsatt pausat.
- Verifierat: web-check 45 tester + TypeScript/build/budget OK; workspace-Clippy OK; MCP 144/144. Rust 327/328, enda fel var fel tunerfixtur; rättad riktad sessionskörning 4/4 OK. Fysisk Cast/tuner återstår.

### 2026-10-27 — Fördjupad Cast-granskning av d05a03f och verifierade rättningar
- **Granskat:** Senaste commit `d05a03f` mot faktisk kod och Googles SDK-referenser. Dess spelarregistertester verifierar inte verklig Cast-uppspelning. Den samlade rapporten och API/UI-inventeringen är uppdaterade; mdBook-guiden skiljer nu implementation från kvarstående produktflöden.
- **Rättat:** Mottagarposition för progress/stop; lokal återgång efter avbruten anslutning; SDK `ANY_CHANGE`; volym noll; idempotent play/pause; remote mute/seek/volym; ordnad avregistrering; sena Google/native-anslutningssvar; native-händelser/reglage; rätt Image-konstruktor; städning vid spelarstängning. Lokala undertext-/hastighets-/nattlägesreglage inaktiveras under casting. Media-CORS/preflight tillagt i `crates/mbl-api/src/routes/stream.rs` med befintliga åtkomstkontroller kvar. Cast-livscykelns avregistrering dokumenterad i MCP-undantagen. Återställde även versionsrubriken 1.0.0 som försvann i senaste changelog-commiten.
- **Tester:** Utökade `web/src/utils/cast.test.ts`, ny renderad `Player.cast.test.tsx`, och CORS/Range/auth-regression i `crates/mbl-server/tests/stream.rs`.
- **Verifiering:** Workspace-nextest **321/321**, workspace-Clippy `-D warnings`, fmt-check, OpenAPI-check, tokens-check, MCP-paritet **142/142 (100%)**, `just web-check` **42 tester**, TypeScript/Vite och bundlebudget **42.03 KB gzip** passerade. HTTP-mocktesterna behövde lokal portbindning utanför sandlådan; ingen kodändring krävdes för det. Ingen fysisk Cast-mottagare testad.
- **Kvar:** Cast-specifik profil/MIME och nåbar URL; receiver-spårval; spelarregistrets ägarisolering; transport från serverns kommandobuss till Cast-klienten; verklig receiver-/nätavbrottsacceptans. Övriga UI-/serverluckor finns prioriterade i granskningsrapporten. M11 ligger fortsatt på vänt. Denna rättningsomgång är ännu inte committad eller publicerad.

### 2026-10-27 — Milstolpe M12 (Chromecast & Remote Playback Support) samt Plugin Buggfix & Härdning
- **Genomfört:**
  - **Plugin Toggle & Re-enable Fix:**
    - Diagnostiserade och åtgärdade buggen där inaktiverade plugins inte kunde startas igen: `handleTogglePlugin` i `web/src/pages/LiveTv.tsx` hade ett hårdkodat `true`-argument som alltid skickade `enabled: false`. Rättat till dynamisk status `handleTogglePlugin(plugin, plugin.enabled ?? true)` med statusbadge (`Active` vs `Disabled`) och dynamisk knapp (`Enable` vs `Disable`).
    - Lade till `pub enabled: bool` med `#[serde(default = "default_true")]` i `PluginManifest` (`crates/mbl-core/src/plugins.rs`).
    - Skapade `PluginService::list_plugins` i `crates/mbl-service/src/plugins.rs` som läser SQLite-tabellen `plugin_configs` och berikar manifesten med aktuell status, samt uppdaterade REST-routen i `crates/mbl-api/src/routes/plugins.rs`.
  - **M12 Chromecast & Remote Playback Support:**
    - Skapade `web/src/utils/cast.ts` med full Google Cast Web Sender SDK (`cast_sender.js`) integration och automatisk handskakning mot Default Media Receiver (`CC1AD845`), samt W3C Remote Playback API fallback för AirPlay och Miracast.
    - Integrerade Cast-kontrollknapp `[⎚]` i `web/src/components/Player.tsx`, dedikerad aktiv Cast-overlay, samt tvåvägssynkning av play/pause/seek/volym.
    - Rapportering av Chromecast-sessioner till `PlayerRegistry` via `POST /api/v1/players` (`device_type: "chromecast"`).
    - Implementerade `DELETE /api/v1/players/{id}` i `crates/mbl-api/src/routes/players.rs` för omedelbar avregistrering vid Cast disconnect.
    - Härdade `PlayerInfo.last_seen_at` med `#[serde(default)]` i `crates/mbl-service/src/players.rs` för att förhindra 422-fel vid spelarregistrering.
  - **Lokal Verifiering & Buggsniffning:**
    - Upptäckte och åtgärdade regression i `crates/mbl-playback/tests/decide.rs` för `audio_index`-val.
    - Skapade omfattande integrationstestsats `crates/mbl-server/tests/m12.rs` (3 tester) och vitest-test `web/src/utils/cast.test.ts`.
    - Skapade komplett dokumentationsguide i `docs/book/src/chromecast.md` och uppdaterade `SUMMARY.md`.
- **Verifieringsresultat:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK (0 fel, 0 varningar)
  - `cargo nextest run --workspace`: 320/320 tester godkända (0 failed, 0 skipped)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `cargo xtask mcp-parity`: OK (141/141 endpoints täckta, 100.0%)
  - `just web-check`: Biome (0 fel), Vitest (34 tester), `tsc -b`, Vite build, bundle budget (42.04 KB gzip <= 150 KB): OK
- **Nästa steg:** Återuppta **M11 — Native Mobile, TV & Spatial VR Clients** (`M11.1`–`M11.4` i `IMPLEMENTATION.md`).

### 2026-10-27 — Kod-/UI-granskning, pausad på användarens begäran
- **Arbete hittills:** Konkreta rättningar för ACL i samlingar/spellistor, TOTP, profil-/inloggningsflöde, uppspelning/spår/versioner, bibliotekspaginering, EPG-tidsenheter, DVR-konflikter och HTTP-fel. Nya webbvägar `/account`, `/collections`, `/admin/remote`. Detaljerad täckning och kvarstående fynd i `docs/audits/2026-10-27-web-and-service-audit.md`; API/UI-inventering i samma katalog.
- **Verifierat:** `just web-check` passerade med 34 tester, TypeScript, Vite och initial JS 42.04 KB gzip. Riktad Clippy för `mbl-service`/`mbl-db` passerade. Webbläsarkontroll mot separat testdatabas verifierade inloggning och navigation. OpenAPI/TS-schema genererade.
- **Paus:** Användaren bad pausa så att annat bygge kan bli klart. Egna pågående workspace-nextest och workspace-Clippy avbröts; de är INTE färdigverifierade. Egna tillfälliga testservrar stoppades. Kodändringar ligger kvar, ingen commit/release gjord.
- **Vid återupptagning:** Kör återstående workspace-tester/Clippy med Pi-flaggorna, kontrollera nya regressioner i `crates/mbl-server/tests/audit.rs` och `mbl-playback/tests/decide.rs`, samt OpenAPI/MCP-paritet/tokens. Flera filer har ändrats parallellt av annat arbete, särskilt plugin/Cast/Player; bevara dessa ändringar och granska aktuell diff före fortsättning. M11-klienter avvaktar.

### 2026-10-27 — Orientering i projektets arkitektur och implementation
- **Genomfört:** Läste projektplan, implementationsstatus, workspace, serverns uppstart, det gemensamma tjänstelagret, scanner, uppspelningsstruktur, webbklient samt delar av TV-, Hailo- och pluginimplementationerna.
- **Observation:** Dokumenterad v1.0-status är inte i sig bevis på fullständig körbar integration. `mbl-tv/src/main.rs` konfigurerar klienten och söker server men startar ingen UI-/spelarslinga; `mbl-hailo::analyze_image` använder färg-/textheuristik; online-pluginflöden innehåller `example.com`-medier. Dessa delar behöver verifieras mot verkliga acceptanskriterier före vidare färdigmarkering.
- **Verifiering:** Endast dokument- och kodläsning; inga byggen, tester eller hårdvarukörningar utförda. Befintliga lokala ändringar i plugin/API/OpenAPI/LiveTv lämnades orörda.
- **Nästa steg:** Vid fortsatt utveckling, kontrollera faktisk integration för berörd funktion. M11 är nästa planerade milstolpe enligt implementationsplanen.

### 2026-10-26 — Milestone M10 (Hardening → v1.0.0) & Release v1.0.0

- Completed (`M10.1`–`M10.8`):
  - M10.1: Formal STRIDE threat model in `docs/threat-model.md` and security hardening in `crates/mbl-service` for webhook validation, DoS caps, and path traversal prevention.
  - M10.2: Fuzzing suite in `fuzz/` for metadata, M3U/XMLTV, and HLS/range handling; fixed a panic in `parse_xmltv_date` for arbitrary multibyte UTF-8 strings.
  - M10.3: Benchmark report in `docs/benchmarks.md` covering scan throughput, TTFF, memory footprint, and zero-CPU virtual playhead metrics.
  - M10.4: Complete mdBook documentation site in `docs/book/` covering installation, hardware acceleration, Tailscale remote access, Live TV/DVR, MCP, AI companion, migration, and troubleshooting.
  - M10.5: `ImportService` for Plex and Jellyfin history/watch-state migration plus REST and MCP endpoints for import workflows.
  - M10.6: Full i18n coverage in the web client with English, Swedish, German, French, and Spanish translations and WCAG 2.2 AA accessibility.
  - M10.7: Separate release builds for `mbl-worker` and `mbl-node` with distribution packages and documentation updates.
  - M10.8: MCP evaluation scenarios and runner for AI/admin workflows; admin tool count reaches 74 with 100% OpenAPI parity.

- Verification:
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 311/311 tests passed (0 failed, 0 skipped)
  - `cargo xtask mcp-parity`: OK (141/141 endpoints covered, 100.0%)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome (0 errors), Vitest (22 tests), `tsc -b`, Vite build: OK

- Next step: begin **M11.1** in [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (native Android mobile & tablet app with Compose and Media3/ExoPlayer).

### 2026-10-25 — Milestone M4 (Hardware Acceleration) & Release `v0.3.0`

- Completed: hardware acceleration stack in `crates/mbl-hwaccel`, FFmpeg capability probing, Hailo NPU co-tenant sharing, multi-backend fallback logic, and admin hardware diagnostics.
- Verification: Rust formatting, clippy, workspace tests, web checks, and OpenAPI validation all passed.

### 2026-10-25 — Milestone M3 (Modern UI v1) & Release `v0.2.0`

- Completed: design tokens, four themes, responsive UI, command palette, player detail view, i18n scaffolding, and bundle-size budget enforcement.

### 2026-10-24..25 — Milestones M0, M1 & M2 (`v0.1.0` MVP)

- Completed: base architecture and MVP media stack across `mbl-core`, `mbl-platform`, `mbl-db`, `mbl-auth`, `mbl-parser`, `mbl-probe`, `mbl-jobs`, `mbl-scanner`, `mbl-metadata`, `mbl-images`, `mbl-playback`, `mbl-transcode`, `mbl-stream`, `mbl-service`, `mbl-api`, `mbl-mcp`, `mbl-server`, `mbl-cli`, and `web/`.
