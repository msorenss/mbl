-- M5: Markers (intro/credits), Trickplay sprite indexes, Optimized version rules, Quick Connect, and Hailo NPU scene embeddings.

CREATE TABLE markers (
    id INTEGER PRIMARY KEY,
    item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    part_id INTEGER REFERENCES media_parts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    title TEXT,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    source TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    created_at INTEGER NOT NULL,
    UNIQUE(item_id, kind, start_ms)
);
CREATE INDEX ix_markers_item ON markers(item_id, start_ms);

CREATE TABLE trickplay (
    part_id INTEGER PRIMARY KEY REFERENCES media_parts(id) ON DELETE CASCADE,
    interval_ms INTEGER NOT NULL DEFAULT 10000,
    tile_width INTEGER NOT NULL DEFAULT 320,
    tile_height INTEGER NOT NULL DEFAULT 180,
    columns INTEGER NOT NULL DEFAULT 10,
    rows INTEGER NOT NULL DEFAULT 10,
    frame_count INTEGER NOT NULL,
    sprite_count INTEGER NOT NULL,
    manifest_json TEXT NOT NULL,
    generated_ms INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE optimization_rules (
    id INTEGER PRIMARY KEY,
    library_id INTEGER REFERENCES libraries(id) ON DELETE CASCADE,
    item_id INTEGER REFERENCES media_items(id) ON DELETE CASCADE,
    profile TEXT NOT NULL,
    night_window_only INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'queued',
    version_id INTEGER REFERENCES media_versions(id) ON DELETE SET NULL,
    output_path TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX ix_opt_rules_item ON optimization_rules(item_id);
CREATE INDEX ix_opt_rules_lib ON optimization_rules(library_id);

CREATE TABLE quick_connect_codes (
    code TEXT PRIMARY KEY,
    secret TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL,
    client_name TEXT NOT NULL,
    user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
    session_token TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE scene_embeddings (
    id INTEGER PRIMARY KEY,
    item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    part_id INTEGER REFERENCES media_parts(id) ON DELETE CASCADE,
    timestamp_ms INTEGER NOT NULL DEFAULT 0,
    tags TEXT NOT NULL DEFAULT '[]',
    caption TEXT NOT NULL DEFAULT '',
    embedding_json TEXT NOT NULL,
    backend TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(item_id, timestamp_ms)
);
CREATE INDEX ix_scene_embeddings_item ON scene_embeddings(item_id, timestamp_ms);
