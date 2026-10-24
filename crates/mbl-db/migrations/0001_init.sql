-- MVP schema (IMPLEMENTATION.md Appendix B). Append-only: never edit after commit.

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name  TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    is_admin      INTEGER NOT NULL DEFAULT 0,
    preferences   TEXT NOT NULL DEFAULT '{}',
    created_at    INTEGER NOT NULL
);

CREATE TABLE sessions (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash   BLOB NOT NULL UNIQUE,
    device_name  TEXT,
    client_name  TEXT,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL
);

CREATE TABLE libraries (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    settings   TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
);

CREATE TABLE library_paths (
    id           INTEGER PRIMARY KEY,
    library_id   INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path         TEXT NOT NULL UNIQUE,
    is_network   INTEGER NOT NULL DEFAULT 0,
    last_scan_at INTEGER
);

CREATE TABLE media_items (
    id                      INTEGER PRIMARY KEY,
    library_id              INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    kind                    TEXT NOT NULL,
    parent_id               INTEGER REFERENCES media_items(id) ON DELETE CASCADE,
    title                   TEXT NOT NULL,
    sort_title              TEXT NOT NULL,
    original_title          TEXT,
    year                    INTEGER,
    idx                     INTEGER,
    parent_idx              INTEGER,
    originally_available_at TEXT,
    summary                 TEXT,
    tagline                 TEXT,
    content_rating          TEXT,
    studio                  TEXT,
    rating                  REAL,
    audience_rating         REAL,
    duration_ms             INTEGER,
    tmdb_id                 INTEGER,
    tvdb_id                 INTEGER,
    imdb_id                 TEXT,
    match_confidence        REAL,
    needs_review            INTEGER NOT NULL DEFAULT 0,
    locked_fields           TEXT NOT NULL DEFAULT '[]',
    added_at                INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL,
    metadata_refreshed_at   INTEGER
);
CREATE INDEX ix_items_lib_kind_added ON media_items(library_id, kind, added_at DESC);
CREATE INDEX ix_items_parent ON media_items(parent_id, idx);
CREATE INDEX ix_items_tmdb ON media_items(tmdb_id);

CREATE TABLE media_versions (
    id                INTEGER PRIMARY KEY,
    item_id           INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    edition           TEXT,
    container         TEXT,
    bitrate           INTEGER,
    width             INTEGER,
    height            INTEGER,
    video_codec       TEXT,
    video_profile     TEXT,
    bit_depth         INTEGER,
    hdr               TEXT,
    audio_codec       TEXT,
    audio_channels    INTEGER,
    is_optimized      INTEGER NOT NULL DEFAULT 0,
    optimized_profile TEXT
);

CREATE TABLE media_parts (
    id            INTEGER PRIMARY KEY,
    version_id    INTEGER NOT NULL REFERENCES media_versions(id) ON DELETE CASCADE,
    path          TEXT NOT NULL UNIQUE,
    size          INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    quick_hash    BLOB,
    duration_ms   INTEGER,
    probe         TEXT NOT NULL,
    chapters      TEXT NOT NULL DEFAULT '[]',
    part_index    INTEGER NOT NULL DEFAULT 0,
    missing_since INTEGER
);
CREATE INDEX ix_parts_hash ON media_parts(quick_hash);

-- Multi-episode files: an episode item may share a version via this link.
CREATE TABLE item_versions (
    item_id    INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    version_id INTEGER NOT NULL REFERENCES media_versions(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, version_id)
);

CREATE TABLE streams (
    id             INTEGER PRIMARY KEY,
    part_id        INTEGER NOT NULL REFERENCES media_parts(id) ON DELETE CASCADE,
    idx            INTEGER NOT NULL,
    kind           TEXT NOT NULL,
    codec          TEXT NOT NULL,
    profile        TEXT,
    language       TEXT,
    title          TEXT,
    is_default     INTEGER NOT NULL DEFAULT 0,
    is_forced      INTEGER NOT NULL DEFAULT 0,
    external_path  TEXT,
    width          INTEGER,
    height         INTEGER,
    bit_depth      INTEGER,
    hdr            TEXT,
    channels       INTEGER,
    channel_layout TEXT
);
CREATE INDEX ix_streams_part ON streams(part_id, idx);

CREATE TABLE people (
    id        INTEGER PRIMARY KEY,
    name      TEXT NOT NULL,
    tmdb_id   INTEGER UNIQUE,
    thumb_url TEXT
);

CREATE TABLE item_people (
    item_id   INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id),
    role      TEXT NOT NULL,
    character TEXT,
    ord       INTEGER,
    PRIMARY KEY (item_id, person_id, role)
);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    UNIQUE (kind, name)
);

CREATE TABLE item_tags (
    item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id),
    PRIMARY KEY (item_id, tag_id)
);

CREATE TABLE artwork (
    id         INTEGER PRIMARY KEY,
    item_id    INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    source_url TEXT,
    local_hash TEXT,
    width      INTEGER,
    height     INTEGER,
    blurhash   TEXT,
    palette    TEXT,
    selected   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE watch_state (
    user_id        INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id        INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    view_offset_ms INTEGER NOT NULL DEFAULT 0,
    view_count     INTEGER NOT NULL DEFAULT 0,
    last_viewed_at INTEGER,
    PRIMARY KEY (user_id, item_id)
);
CREATE INDEX ix_watch_recent ON watch_state(user_id, last_viewed_at DESC);

CREATE TABLE watch_history (
    id         INTEGER PRIMARY KEY,
    user_id    INTEGER NOT NULL,
    item_id    INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    stopped_at INTEGER,
    method     TEXT,
    client     TEXT,
    network    TEXT
);

CREATE TABLE jobs (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL,
    status     TEXT NOT NULL,
    priority   INTEGER NOT NULL,
    progress   REAL NOT NULL DEFAULT 0,
    attempts   INTEGER NOT NULL DEFAULT 0,
    run_after  INTEGER NOT NULL,
    error      TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX ix_jobs_ready ON jobs(status, priority, run_after);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE VIRTUAL TABLE items_fts USING fts5(
    title, original_title, content = 'media_items', content_rowid = 'id'
);

CREATE TRIGGER items_ai AFTER INSERT ON media_items BEGIN
    INSERT INTO items_fts (rowid, title, original_title)
    VALUES (new.id, new.title, new.original_title);
END;

CREATE TRIGGER items_ad AFTER DELETE ON media_items BEGIN
    INSERT INTO items_fts (items_fts, rowid, title, original_title)
    VALUES ('delete', old.id, old.title, old.original_title);
END;

CREATE TRIGGER items_au AFTER UPDATE OF title, original_title ON media_items BEGIN
    INSERT INTO items_fts (items_fts, rowid, title, original_title)
    VALUES ('delete', old.id, old.title, old.original_title);
    INSERT INTO items_fts (rowid, title, original_title)
    VALUES (new.id, new.title, new.original_title);
END;
