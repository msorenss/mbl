# 0002. SQLite with a single writer

- Status: Accepted
- Date: 2026-10-23

## Context
MBL targets a Raspberry Pi 5 as its reference platform and must be zero-configuration. Library scans write heavily while the UI reads concurrently. SQLite allows only one writer at a time, and naive pools produce `SQLITE_BUSY` storms.

## Decision
- SQLite in WAL mode is the default database, accessed via `sqlx` with compile-time checked queries (`.sqlx/` offline data is committed).
- Use two pools: `Db.write` (exactly 1 connection) for all writes, and `Db.read` (N read-only connections) for reads.
- Bulk writes are batched in transactions of ≤ 500 rows.
- Pragmas: `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`, `temp_store=MEMORY`, and `cache_size` scaled to RAM.
- Migrations are append-only. A `VACUUM INTO` backup is taken before migrating an existing database.
- Postgres remains an optional future feature.

## Consequences
- No database server to install, and backups are a single file.
- Write throughput is serialized. This is fine for a media server, but long write transactions must be avoided.
- Read connections cannot accidentally write (`read_only`), which enforces the pattern.
- Each query needs a matching `.sqlx` entry: run `just sqlx-prepare` after changing queries.
