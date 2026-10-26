# Architecture & Philosophy

MBL is organized as a Cargo workspace with clean separation of concerns across focused, single-responsibility crates.

## Workspace Layout

```
mbl/
├── crates/
│   ├── mbl-core/         # Fundamental IDs, types, models, errors and utilities
│   ├── mbl-db/           # SQLite migrations, schema definitions, and repository queries
│   ├── mbl-auth/         # Argon2id, JWT, API tokens, OAuth 2.1 PKCE, TOTP 2FA
│   ├── mbl-scanner/      # Filesystem scanner, inotify watcher, media parser
│   ├── mbl-probe/        # ffprobe wrapper and media format inspection
│   ├── mbl-metadata/     # TMDB, TVDB, MusicBrainz, OpenSubtitles clients & NFO parser
│   ├── mbl-hwaccel/      # Hardware fingerprinting: VAAPI, QSV, NVENC, V4L2, AMF, D3D11
│   ├── mbl-transcode/    # FFmpeg pipeline builder, HDR tone mapping, ABR ladder
│   ├── mbl-stream/       # HLS fMP4 segmenter, range streaming, audio transcode
│   ├── mbl-service/      # Domain business logic: library, playback, users, livetv, import
│   ├── mbl-api/          # Axum HTTP routes, WebSocket hubs, OpenAPI 3.1 generator
│   ├── mbl-mcp/          # Model Context Protocol server, tools, audit logging, OAuth
│   ├── mbl-remote/       # Tailscale LocalAPI client, network classifier, bandwidth arbiter
│   ├── mbl-analysis/     # EBU R128 loudness, speech VAD, subtitle auto-sync
│   ├── mbl-hailo/        # Hailo-8 / Hailo-10H NPU cooperative scheduler & Ollama bridge
│   ├── mbl-worker/       # Standalone transcode worker daemon
│   ├── mbl-node/         # Standalone remote library edge storage daemon
│   └── mbl-server/       # Integrated executable combining server components
├── web/                  # SolidJS frontend, Biome linter, Tailwind CSS, i18n
├── packaging/            # Systemd units, WiX MSI installers, Docker multi-arch
└── fuzz/                 # libfuzzer targets and property-based fuzzing suite
```

## Data Storage Philosophy

- **Single Embedded Database:** MBL uses SQLite in WAL2 mode. No external PostgreSQL or MySQL dependencies required.
- **ACID Transactions:** Migrations and metadata mutations execute in explicit transactions.
- **Zero Foreign Key Violations:** Foreign keys with `ON DELETE CASCADE` ensure spotless data hygiene when media items or libraries are removed.
- **Pragmatic Indexing:** Indexes on external IDs (`tmdb_id`, `imdb_id`), composite titles (`title, year`), and FTS5 full-text virtual tables deliver sub-millisecond search latencies.
