-- Migration 0007: Milestone M8 Distributed Nodes (Transcode Workers & Remote Library Nodes)

-- 1. Transcode Workers (M8.1, M8.2, M8.5, M8.7)
CREATE TABLE workers (
    id                  INTEGER PRIMARY KEY,
    name                TEXT NOT NULL,
    hostname            TEXT,
    mac_address         TEXT,
    auth_token_hash     BLOB NOT NULL UNIQUE,
    cert_fingerprint    TEXT,
    capabilities        TEXT NOT NULL DEFAULT '{}',
    max_sessions        INTEGER NOT NULL DEFAULT 1,
    status              TEXT NOT NULL DEFAULT 'offline',
    last_heartbeat_at   INTEGER,
    path_mappings       TEXT NOT NULL DEFAULT '[]',
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);
CREATE INDEX ix_workers_status ON workers(status);

CREATE TABLE worker_pairing_codes (
    code        TEXT PRIMARY KEY,
    created_by  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at  INTEGER NOT NULL
);
CREATE INDEX ix_worker_pairing_codes_expires ON worker_pairing_codes(expires_at);

-- 2. Standalone Library Nodes / Federated Storage Nodes (M8.10, M8.11)
CREATE TABLE remote_nodes (
    id                  INTEGER PRIMARY KEY,
    name                TEXT NOT NULL,
    base_url            TEXT NOT NULL,
    auth_token_hash     BLOB NOT NULL UNIQUE,
    status              TEXT NOT NULL DEFAULT 'offline',
    last_seen_at        INTEGER,
    capabilities        TEXT NOT NULL DEFAULT '{}',
    created_at          INTEGER NOT NULL
);
CREATE INDEX ix_remote_nodes_status ON remote_nodes(status);

CREATE TABLE remote_node_libraries (
    id                  INTEGER PRIMARY KEY,
    node_id             INTEGER NOT NULL REFERENCES remote_nodes(id) ON DELETE CASCADE,
    remote_library_id   INTEGER NOT NULL,
    name                TEXT NOT NULL,
    kind                TEXT NOT NULL,
    item_count          INTEGER NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);
CREATE INDEX ix_remote_node_libraries_node ON remote_node_libraries(node_id);

CREATE TABLE remote_node_pairing_codes (
    code        TEXT PRIMARY KEY,
    created_by  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at  INTEGER NOT NULL
);
CREATE INDEX ix_remote_node_pairing_codes_expires ON remote_node_pairing_codes(expires_at);
