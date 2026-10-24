set windows-shell := ["pwsh", "-NoLogo", "-Command"]

sqlx_db := "sqlite://.dev/sqlx.db"

default:
    @just --list

# Format all Rust code
fmt:
    cargo fmt --all

# Everything that must pass before a task is done
check: fmt-check clippy test web-check

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo nextest run --workspace --no-tests=pass

web-check:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -d web ]; then
        pnpm -C web install --frozen-lockfile
        pnpm -C web lint
        pnpm -C web test
        pnpm -C web build
    fi

# Full CI-equivalent run (`cargo xtask openapi-check` is added in M1.9)
ci: check
    cargo deny check
    just sqlx-check

# Run the server with dev config/data in ./.dev
dev:
    mkdir -p .dev/data
    MBL_LOG__LEVEL=debug MBL_PATHS__DATA_DIR=.dev/data cargo run -p mbl-server -- --config .dev/mbl.toml

web-dev:
    pnpm -C web dev

# Create/migrate the dev DB used by sqlx compile-time checks
sqlx-db:
    mkdir -p .dev
    DATABASE_URL={{sqlx_db}} sqlx database create
    DATABASE_URL={{sqlx_db}} sqlx migrate run --source crates/mbl-db/migrations

# Regenerate .sqlx offline query data (commit the result)
sqlx-prepare: sqlx-db
    DATABASE_URL={{sqlx_db}} cargo sqlx prepare --workspace -- --all-targets

sqlx-check: sqlx-db
    DATABASE_URL={{sqlx_db}} cargo sqlx prepare --workspace --check -- --all-targets

gen-media *ARGS:
    cargo xtask gen-media {{ARGS}}

build-pi:
    cargo zigbuild --release --target aarch64-unknown-linux-gnu -p mbl-server

deb:
    cargo deb -p mbl-server
