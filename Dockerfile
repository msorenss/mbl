# syntax=docker/dockerfile:1

# --- Stage 1: Build Web Client ---
FROM node:22-bookworm-slim AS web-builder
WORKDIR /app/web
RUN corepack enable && corepack prepare pnpm@latest --activate
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# --- Stage 2: Build Rust Server ---
FROM rust:1.85-bookworm AS rust-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY xtask/ ./xtask/
COPY packaging/ ./packaging/
COPY docs/ ./docs/
COPY --from=web-builder /app/web/dist ./web/dist
ENV SQLX_OFFLINE=true
RUN cargo build --release -p mbl-server

# --- Stage 3: Runtime Environment ---
FROM ubuntu:24.04 AS runtime

LABEL org.opencontainers.image.title="MBL Media Server" \
      org.opencontainers.image.description="Fast, lightweight personal media server for Raspberry Pi 5 and home servers" \
      org.opencontainers.image.authors="MBL Contributors <dev@mbl.media>" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"

ENV DEBIAN_FRONTEND=noninteractive \
    PUID=1000 \
    PGID=1000 \
    MBL_PATHS__DATA_DIR=/var/lib/mbl \
    MBL_SERVER__BIND=0.0.0.0 \
    MBL_SERVER__PORT=8420

RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    ca-certificates \
    gosu \
    tini \
    tzdata \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /var/lib/mbl /etc/mbl /media

COPY packaging/docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

COPY --from=rust-builder /app/target/release/mbl-server /usr/bin/mbl-server
RUN chmod 755 /usr/bin/mbl-server

EXPOSE 8420
VOLUME ["/var/lib/mbl", "/etc/mbl", "/media"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["/usr/bin/mbl-server", "--config", "/etc/mbl/mbl.toml"]
