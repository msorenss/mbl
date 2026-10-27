# AGENTS.md — Agent Handoff, Environment Guide & Work Log
This document lives at the repository root so every AI agent, contributor, and automation tool can find the current status, environment-specific rules, architectural decisions, and handoff notes quickly.

---
## 1. Current status (quick overview)
- Latest release: `v1.0.0` (published 2026-10-26, M10 completed — General Availability)
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
