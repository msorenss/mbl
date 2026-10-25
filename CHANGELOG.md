# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-10-25

### Added
- M4 Hardware acceleration (`mbl-hwaccel`): platform fingerprinting across Linux (DRM/V4L2/Hailo/Jetson/Raspberry Pi) and Windows (DXGI), FFmpeg capability probing (`-hwaccels`, `-encoders`, `-decoders`, `-filters`), and functional self-testing cached in `hw_capabilities` (`0003_hw_capabilities.sql`).
- Hardware transcode backends in `mbl-transcode` for VAAPI, Intel QSV, NVIDIA NVENC, Raspberry Pi (`V4l2m2m` for Pi 4 and `PiDrmDecode` for Pi 5), and Windows (`D3d11Amf`), plus ordered fallback chains with 5-second early-exit failure detection and 1-hour bad-backend tracking.
- HDR10/HLG-to-SDR BT.709 tone mapping (`tonemap_vaapi`, `vpp_qsv`, `tonemap_cuda`, and software `zscale` + `tonemap=hable`), GPU multi-rendition ABR ladders (`split=N`), and remote HEVC/AV1 output preference on constrained networks.
- Hardware capacity model (`GET /api/v1/system/hw`, `POST /api/v1/system/hw/retest`, `GET /api/v1/system/transcode-capacity`), Admin Hardware & NPU UI (`web/src/pages/admin/Hardware.tsx`), and MCP hardware tools (`get_hw_report`, `get_transcode_capacity`, `run_hw_selftest`) plus `mbl://hw/report` resource.
- Hailo-8 / Hailo-10H NPU detection and cooperative multi-process `VDevice` sharing (`hailo.mode`, `hailo.vdevice_group_id`) for co-tenant setups (such as Frigate, `hailo-ollama`, or GStreamer pipelines), plus `.github/workflows/ffmpeg.yml` custom FFmpeg build pipeline.

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

