# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.8.0] - 2026-10-26

### Added
- M9 Live TV, Channels and Online Streaming Plugins:
  - HDHomeRun tuner discovery on local network via UDP broadcast (`255.255.255.255:65001`), device inspection, lineup synchronization, and channel mapping.
  - M3U and M3U8 playlist parser (`#EXTM3U`, `#EXTINF`) extracting `tvg-id`, `tvg-logo`, `tvg-chno`, callsigns, and stream URLs.
  - Streaming XMLTV parser (`parse_xmltv_streaming`) with chunked event reading via `quick-xml`, parsing programme schedules, titles, descriptions, genres, ratings, and `xmltv_ns` season/episode numbering.
  - Live playback engine (`GET /api/v1/livetv/stream/{id}`) with tuner concurrency tracking and conflict management.
  - DVR recording scheduler (`/api/v1/livetv/recordings`, `/api/v1/livetv/rules`) supporting one-off recordings, series rules with new-only and season filters, pre/post-padding, tuner conflict checks, and retention keep policies.
  - Two-axis EPG TV Guide grid API (`/api/v1/livetv/epg`) with now/next program tracking and guide search (`/api/v1/livetv/epg/search`).
  - Online media streaming plugin provider framework (`mbl-plugins` / `PluginService`) with built-in feeds for YouTube, Spotify, and Web/K-Drama, item browsing, and stream resolution mapped to `ItemSummary`.
  - Virtual 24/7 Linear TV channels (`/api/v1/livetv/virtual-channels`) with mathematical zero-CPU live playhead calculation (`/api/v1/livetv/virtual-channels/{id}/playhead`) and rolling synthetic EPG generation.
  - SolidJS Live TV UI (`/livetv`) featuring 4 tabs: Interactive 2-axis EPG TV Guide with program detail modal & DVR schedule buttons; Tuners & Sources management with HDHomeRun auto-discovery and M3U/XMLTV import modals; DVR Recordings & Series Rules management; and Virtual 24/7 Channels with live playhead progress & online Streaming Plugins browser.
  - 5 new MCP tools (`search_guide`, `list_recordings`, `schedule_recording`, `create_series_rule`, `cancel_recording`), expanding the admin toolset to 72 tools with 100% OpenAPI coverage parity.

## [0.7.0] - 2026-10-26

### Added
- M8 Distributed nodes (Transcode workers & Remote library nodes):
  - Worker protocol specification (`docs/worker-protocol.md`) and typed data structures (`WorkerJobSpec`, `WorkerCapabilities`, `WorkerHeartbeat`, `WorkerControl`) in `mbl-core`.
  - Distributed transcode worker CLI binary `mbl-worker` with pairing code registration, hardware capability probing via `mbl-hwaccel`, periodic heartbeat loop, and pause-while-gaming logic.
  - Signed source media range chunk streaming (`/api/v1/stream/worker/source/{partId}`) with HMAC-SHA256 source tokens (`WorkerSourceTokenClaims`) guarding against directory traversal.
  - Encoded segment upload endpoint (`PUT /api/v1/stream/hls/{sessionId}/segments/{filename}`) storing chunks directly in the central server's session directory for zero-latency client serving.
  - Scheduler integration with Wake-on-LAN (UDP magic packet on port 9) to awaken sleeping GPU workers, idle worker selection, and seamless local fallback on worker loss.
  - Batch offload support for optimized versions, trickplay scrubbing sheets, and intro analysis with worker time windows and dry-run estimation.
  - Windows worker tray application support with pause-while-gaming GPU load threshold detection.
  - Admin Web UI for distributed nodes (`/admin/workers`) featuring pairing code modals for workers and storage nodes, active session and capability views, and Wake-on-LAN and revocation controls.
  - 5 new MCP tools (`list_workers`, `wake_worker`, `pair_worker`, `revoke_worker`, `list_remote_nodes`) expanding the admin toolset to 67 tools with 100% OpenAPI coverage parity.
  - Standalone Library Node daemon `mbl-node` for federated storage (NAS, remote mini-PCs, or secondary servers) with one-time pairing and periodic delta catalog synchronization (`POST /api/v1/nodes/{id}/sync-catalog`).
  - Remote Node Streaming & Smart Handover (`GET /api/v1/nodes/{id}/stream/handover/{itemId}`) detecting client IP to grant direct local play handover URLs on LAN subnets while maintaining relay fallbacks for WAN connections.

### Changed
- Package the standalone `mbl-worker` and `mbl-node` binaries in Debian and Windows MSI/ZIP releases.

## [0.6.0] - 2026-10-26

### Added
- M7 Media breadth:
  - Music scanning (`mbl-scanner`) with `lofty` metadata extraction (artist, album, track, disk, lyrics, ReplayGain, MusicBrainz IDs) and rate-limited MusicBrainz + Cover Art Archive client (`mbl-metadata::musicbrainz`).
  - EBU R128 audio loudness analysis (`mbl-analysis::loudness`) and progressive audio transcode streaming endpoint (`/api/v1/stream/audio/{partId}` in Opus, AAC, or MP3 with direct stdout piping).
  - OpenSubsonic API compatibility layer (`/rest/*`) supporting standard Subsonic mobile and desktop apps (Symfonium, Feishin, DSub, etc.).
  - Photos timeline and EXIF extraction (`mbl-scanner`, `/api/v1/libraries/{id}/timeline`) grouping photos by year and month.
  - Collections and Playlists (`/api/v1/collections`, `/api/v1/playlists`) with automatic and manual item curation.
  - Item metadata field locking (`item_metadata_locks`) preventing automatic scrapers from overwriting manually verified fields.
  - OpenSubtitles REST client (`mbl-metadata::opensubtitles`) for automatic and on-demand subtitle retrieval.
  - DLNA / UPnP Media Server (`mbl-remote::dlna`) with root device description XML, ContentDirectory DIDL-Lite browse trees, and Smart TV renderer profiles (Samsung, LG WebOS, Sony Bravia).
  - Player Registry and Remote Control (`mbl-service::players`, `/api/v1/players`) tracking web, TV, DLNA, and Cast players.
  - Webhook dispatch system (`/api/v1/webhooks`) with HMAC-SHA256 signature verification for cinema automation and notification bots.
  - Home Assistant MQTT Integration (`mbl-remote::mqtt`) with MQTT Discovery payloads for media players and diagnostic sensors, plus playback lifecycle event triggers.
  - AI Watch Companion protocol (`mbl://sessions/{id}/live-context`, `/api/v1/companion/sessions/{id}/*`) enabling AI agents (e.g. Hermes) to co-watch movies with users, receive live VTT dialogue cues, send commentary reactions, and explain scenes.
  - Subtitle Auto-Sync (`mbl-analysis::subsync`) with voice energy envelope detection and dynamic programming alignment for drifting subtitles.
  - Scene X-Ray on Pause, Spoiler-Free "Previously On..." AI recaps bounded strictly by watch history, Binge Sleep Guard, and shareable clip/GIF creation logic.
  - Movie Night Matchmaker ("Swipe to Match") real-time session engine (`/api/v1/matchmaker/sessions`).
  - Native Media Requests (`/api/v1/requests`) with Radarr/Sonarr integration, missing episodes inspector, and hardlink-aware storage analysis (`st_nlink`).
  - 20 new MCP tools (expanding total tool count to 62, 100% OpenAPI coverage parity), 5 new prompt templates (`diagnose_playback`, `library_health_check`, `optimize_for_remote`, `weekly_report`, `curate_collection`), and `resources/subscribe` live streaming.
  - Windows WiX MSI packaging script (`packaging/windows/build-msi.ps1`) and GitHub Actions release workflow integration producing `mbl-setup-x64.msi`.

## [0.5.0] - 2026-10-26

### Added
- M6 Users, sharing, remote access, and Windows:
  - Per-user library access control lists (`user_library_access`) enforced via `ItemFilter` across all items, search, and playback routes, treating unpermitted libraries as 404 Not Found.
  - Managed user profiles with parental PIN protection (Argon2id) and age content-rating ceilings (G, PG, PG-13, R, NC-17, TV-MA).
  - Share invites (`invites`, `POST /api/v1/invites`, `GET /api/v1/invites/inspect/{code}`, `POST /api/v1/auth/register-invite`) with expiration, pre-assigned library access, and content rating ceilings.
  - Time-based One-Time Password (TOTP) 2FA (`totp-rs`, RFC 6238) via `/api/v1/auth/totp/setup`, `/verify`, `/disable`, strictly required for administrators connecting over WAN.
  - Network classification (`mbl-remote::network`) distinguishing LAN subnets, Tailscale CGNAT ranges (`100.64.0.0/10`), Loopback, and WAN, with trusted proxy forward header resolution (`X-Forwarded-For`).
  - Remote bandwidth management (`mbl-remote::bandwidth`) with per-client throughput feedback and fair-share allocation under the server's WAN upload limit.
  - Tailscale integration (`mbl-remote::tailscale`) with LocalAPI client for node status, MagicDNS names, cert retrieval, and loopback-verified identity authentication (`POST /api/v1/auth/tailscale`) with spoofing rejection.
  - CGNAT reachability diagnostics (`mbl-remote::reachability`) recommending Tailscale vs. Port-Forwarding/TLS.
  - Admin endpoints guarded by default against WAN connections (`admin_lan_only` policy).
  - Windows packaging: WiX MSI installer (`packaging/windows/mbl.wxs`) with Server, Worker, and TV Client components, Windows service and tray icon support, bundled FFmpeg, Windows Firewall rule, and `winget` manifest (`packaging/windows/mbl.yaml`).
  - Docker images: multi-arch Dockerfile (`packaging/docker/Dockerfile`), VAAPI and NVIDIA hardware-accelerated variants, and `docker-compose.tailscale.yml` with Tailscale sidecar.
  - MCP mutation framework: `MutationOpts { dry_run, confirm }` on all state-modifying tools, requiring explicit confirmation or MCP elicitation for destructive operations.
  - 15 new MCP tools across `Users`, `Remote`, and `Admin` scopes: `list_users`, `create_user`, `update_user_permissions`, `create_invite`, `revoke_sessions`, `delete_user`, `get_remote_access_status`, `run_reachability_test`, `get_bandwidth_usage`, `update_remote_policy`, `get_settings`, `update_settings` (with MCP security immutability), `create_backup`, `list_backups`, `restart_server`, and prompt `setup_remote_access` (expanding admin scope to 42 tools).
  - OAuth 2.1 authorization code flow with PKCE (RFC 7636, S256) and Dynamic Client Registration for MCP clients (`/.well-known/oauth-authorization-server`, `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`).

## [0.4.0] - 2026-10-26

### Added
- M5 Pi 5 excellence + TV client:
  - Thermal and power monitoring (`mbl-platform::thermal`) reading SoC temperature and Raspberry Pi throttling/under-voltage bitmasks, automatically pausing idle jobs above 75 °C and exposing status via `/api/v1/system/health`, MCP `get_health`, and the web Hardware dashboard.
  - Platform profile defaults (`PlatformProfile::defaults`) with tuned concurrency, transcode limits, and cache sizes.
  - Automatic `tmpfs` transcode directory selection (`/run/mbl/transcode` capped at 25% of RAM) on Linux hosts with >= 4 GiB of RAM.
  - Pre-transcoded optimized versions (`mbl-analysis::optimize`, `POST /api/v1/items/{id}/optimize`, `create_optimized_versions` MCP tool) with night-window job execution; `PlaybackService` automatically prefers optimized versions for DirectPlay when the primary version requires transcoding.
  - Trickplay scrub thumbnails (`mbl-analysis::trickplay`): keyframe-only extraction into 320px 10x10 JPEG sprite sheets (`sprite_000.jpg`) with manifest index (`/api/v1/stream/trickplay/{partId}/index.json`).
  - Audio intro detection (`mbl-analysis::intro`): Chromaprint sub-fingerprinting across episodes in a season, chapter name preference, and credits detection (`mbl-analysis::credits`) using silence and black frame heuristics, merged directly into `PlaybackResponse.markers`.
  - `mbl-tv` 10-foot TV client (`crates/mbl-tv`): Slint UI using `mbl-ui-kit` tokens, `linuxkms` for DRM/KMS on Raspberry Pi and `winit` on desktop, embedded `libmpv` with `hwdec=auto-safe`, 2D spatial focus navigation, on-screen keyboard, HDMI-CEC (`libcec`), gamepad (`gilrs`), refresh-rate matching (23.976/24/50/59.94/60 Hz), HDR passthrough, audio passthrough (IEC958), and 6-digit Quick-Connect login flow (`/api/v1/auth/quick-connect`, `/authorize`, `/poll`).
  - Raspberry Pi OS appliance image build stage (`packaging/pi-gen/stage-mbl/`) with TCP BBR + `fq`, volatile journald, log2ram, and optional auto-boot into `mbl-tv`, plus Debian APT repository generation scripts (`packaging/apt/`).
  - Hailo-8 / Hailo-10H NPU multimodal media intelligence (`mbl-hailo`): 64-dimensional L2-normalized scene embeddings, natural-language semantic scene search (`GET /api/v1/ai/search-scenes`, MCP `search_scenes`), visual scene/face tagging, hardware benchmarking (`GET /api/v1/ai/benchmark`), and spoiler-free recaps (`GET /api/v1/ai/recap/{id}`, MCP `get_spoiler_free_recap`) powered by `hailo-ollama` with deterministic fallback.

## [0.3.0] - 2026-10-25

### Added
- M4 Hardware acceleration (`mbl-hwaccel`): platform fingerprinting across Linux (DRM/V4L2/Hailo/Jetson/Raspberry Pi) and Windows (DXGI), FFmpeg capability probing (`-hwaccels`, `-encoders`, `-decoders`, `-filters`), and functional self-testing cached in `hw_capabilities` (`0003_hw_capabilities.sql`).
- Hardware transcode backends in `mbl-transcode` for VAAPI, Intel QSV, NVIDIA NVENC, Raspberry Pi (`V4l2m2m` for Pi 4 and `PiDrmDecode` for Pi 5), and Windows (`D3d11Amf`), plus ordered fallback chains with 5-second early-exit failure detection and 1-hour bad-backend tracking.
- HDR10/HLG-to-SDR BT.709 tone mapping (`tonemap_vaapi`, `vpp_qsv`, `tonemap_cuda`, and software `zscale` + `tonemap=hable`), GPU multi-rendition ABR ladders (`split=N`), and remote HEVC/AV1 output preference on constrained networks.
- Hardware capacity model (`GET /api/v1/system/hw`, `POST /api/v1/system/hw/retest`, `GET /api/v1/system/transcode-capacity`), Admin Hardware & NPU UI (`web/src/pages/admin/Hardware.tsx`), and MCP hardware tools (`get_hw_report`, `get_transcode_capacity`, `run_hw_selftest`) plus `mbl://hw/report` resource.
- Hailo-8 / Hailo-10H NPU detection and cooperative multi-process `VDevice` sharing (`hailo.mode`, `hailo.vdevice_group_id`) for co-tenant setups (such as Frigate, `hailo-ollama`, or GStreamer pipelines), plus `.github/workflows/ffmpeg.yml` custom FFmpeg build pipeline.

### Fixed
- Retry temporary Windows file-sharing and locking failures when deleting expired transcode sessions after FFmpeg exits.
- Include the server executable in Linux Debian packages and verify its presence during release builds.

## [0.2.0] - 2026-10-25

### Added
- M3 Modern UI v1: generated design tokens, four themes with user accents, reusable UI components, responsive desktop/mobile/TV navigation, and an updated home experience.
- Refreshed media details and player with playback prediction, quality and subtitle controls, night mode, playback statistics, and Up Next.
- Command palette, profile picker, English translations with Swedish and German scaffolds, and initial bundle-size budget enforcement.

### Fixed
- Normalize HLS output paths on Windows and make filesystem-path tests platform-independent.

### Changed
- Install FFmpeg in Linux, ARM64, and Windows CI test jobs.

## [0.1.0] - 2026-10-25

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

