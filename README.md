# MBL

MBL is an open-source, Plex-inspired media center built in Rust. It is designed around the Raspberry Pi 5, but it also runs well on Windows and Linux desktops and fully leverages Intel and NVIDIA GPU acceleration. The stack is AI-native and ships with a built-in Model Context Protocol (MCP) server.

- Product plan: [PLAN.md](PLAN.md)
- Implementation roadmap: [IMPLEMENTATION.md](IMPLEMENTATION.md)
- Architecture decisions: [docs/adr](docs/adr)
- Documentation site: [docs/book](docs/book)
- Release notes: [CHANGELOG.md](CHANGELOG.md)

## Why MBL

MBL aims to combine the reliability of a self-hosted home media stack with the flexibility of modern streaming, AI-assisted discovery, distributed transcoding, live TV, remote access, and media intelligence.

Key capabilities include:

- Unified library scanning and metadata enrichment
- Direct play and adaptive HLS/fMP4 streaming
- Hardware-accelerated transcoding for Pi, Intel, and NVIDIA
- Local-first remote access with Tailscale-aware networking
- Live TV, DVR, virtual channels, and plugin-based streaming sources
- AI-powered scene search, tagging, recaps, and watch-companion features
- Web UI plus MCP tooling for automation and admin workflows

## Status

Current release: v1.1.0

This release builds on the v1.0.0 foundation with expanded playback, Live TV, DVR, and administration workflows. See [CHANGELOG.md](CHANGELOG.md) for the release summary and [docs/book](docs/book) for the full operational documentation.

## Quick start

Requirements:

- Rust toolchain (managed via `rust-toolchain.toml` and `rustup`)
- [`just`](https://github.com/casey/just)
- `cargo-nextest`
- `cargo-deny`
- `sqlx-cli`
- FFmpeg 6.1+

```sh
just check           # format, clippy, and test suite
just dev             # run the server on http://localhost:8420
just gen-media --quick  # generate the synthetic test library in test-media/out
```

## Repository layout

```text
.
├── crates/            # Rust crates for server, media stack, AI, workers, and plugins
├── docs/              # OpenAPI schema, ADRs, benchmarks, book, and protocol docs
├── evals/             # MCP evaluation scenarios
├── fuzz/              # Fuzz targets and fuzz harnesses
├── packaging/         # Docker, systemd, Pi, and Windows packaging
├── web/               # SolidJS web client
├── AGENTS.md          # Agent handoff and environment notes
├── IMPLEMENTATION.md  # Milestone roadmap and acceptance criteria
├── PLAN.md           # Product and architecture plan
├── README.md          # Project overview and local developer quickstart
├── Cargo.toml         # Workspace configuration
├── justfile           # Common commands for development and verification
└── LICENSE            # AGPL-3.0-or-later
```

## Documentation

- [PLAN.md](PLAN.md) — overall product and architecture plan
- [IMPLEMENTATION.md](IMPLEMENTATION.md) — milestone checklist and execution log
- [docs/book](docs/book) — end-user and operator documentation
- [docs/openapi.json](docs/openapi.json) — generated OpenAPI 3.1 schema
- [docs/worker-protocol.md](docs/worker-protocol.md) — distributed worker protocol specification
- [docs/threat-model.md](docs/threat-model.md) — security model and threat analysis

## License

The server is licensed under AGPL-3.0-or-later; see [LICENSE](LICENSE).

Client SDKs and associated reuse components are covered under MIT; see [LICENSES/MIT.txt](LICENSES/MIT.txt).
