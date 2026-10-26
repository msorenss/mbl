# Introduction to MBL

**MBL** (Modern Media Server) is an open-source, ultra-efficient personal media server designed from the ground up in **Rust** and **SolidJS**.

## Why MBL?

For over a decade, self-hosted media enthusiasts have navigated tradeoffs between bloated legacy systems with proprietary tracking and modern but memory-heavy alternatives. MBL is built upon four foundational pillars:

1. **Native Performance & Efficiency:** Written in memory-safe, asynchronous Rust (`tokio`, `axum`, `sqlx`, `sqlite3`). Idles under 45 MB of RAM and uses zero CPU during direct play and continuous virtual channels.
2. **Raspberry Pi 5 Excellence:** Purpose-tuned for low-power ARM64 single-board computers, including V4L2/DRM hardware acceleration, thermal and under-voltage guards, tmpfs-backed zero-wear transcoding, and NPU offload via Hailo-8 and Hailo-10H.
3. **Distributed Architecture:** Scale compute across machines with `mbl-worker` (offloading transcoding to gaming PCs or GPU servers via Wake-on-LAN) and `mbl-node` (federating remote NAS libraries with Smart Handover).
4. **AI-First & Model Context Protocol (MCP):** Native MCP server with 74 scoped tools, OAuth 2.1 PKCE, real-time event streaming (`resources/subscribe`), and multimodal AI companions for scene search, recap generation, and dialogue synchronization.

## System Components

- **`mbl-server`:** The central media server daemon managing SQLite storage, authentication, REST/WebSocket APIs, scanner, streaming engine, and web UI.
- **`mbl-worker`:** Lightweight transcode worker daemon for Windows and Linux with auto-pairing and pause-while-gaming GPU detection.
- **`mbl-node`:** Autonomous edge storage node for remote NAS machines with catalog delta synchronization and LAN/WAN Smart Handover.
- **`mbl-tv`:** 10-foot TV client with spatial D-pad navigation, HDMI-CEC control, and gamepad support.
- **Web Interface:** Lightning-fast SolidJS single-page application with WCAG 2.2 AA accessibility, i18n localization, and command palette (`Ctrl+K`).
