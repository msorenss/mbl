# MBL

An open source, Plex-inspired media center written in Rust. It is built first for the Raspberry Pi 5, runs well on Windows and Linux PCs, and makes full use of NVIDIA and Intel GPUs. It is AI-native, with a built-in MCP server.

- Product plan: [PLAN.md](PLAN.md)
- Implementation tasks: [IMPLEMENTATION.md](IMPLEMENTATION.md)
- Decisions: [docs/adr](docs/adr)

## Status

Milestone M5 (Pi 5 excellence + TV client) is complete; MBL remains in active development.

## Development

Requirements: Rust (installed automatically from `rust-toolchain.toml` via rustup), [`just`](https://github.com/casey/just), `cargo-nextest`, `cargo-deny`, `sqlx-cli`, FFmpeg ≥ 6.1.

```sh
just check          # fmt, clippy, tests
just dev            # run the server on http://localhost:8420
just gen-media --quick   # generate the synthetic test library in test-media/out
```

## License

The server is licensed under AGPL-3.0-or-later (see [LICENSE](LICENSE)). The client SDKs will be MIT-licensed (see [LICENSES/MIT.txt](LICENSES/MIT.txt)).
