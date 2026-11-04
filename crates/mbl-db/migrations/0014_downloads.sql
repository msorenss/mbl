-- Offline downloads requested by client devices (Android app). `original` serves the
-- source part directly; optimized qualities point at a transcoded file in the cache.
CREATE TABLE downloads (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id     INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    part_id     INTEGER NOT NULL REFERENCES media_parts(id) ON DELETE CASCADE,
    quality     TEXT NOT NULL,
    status      TEXT NOT NULL,
    file_path   TEXT,
    size_bytes  INTEGER,
    error       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (user_id, part_id, quality)
);
CREATE INDEX ix_downloads_user ON downloads(user_id, created_at DESC);
CREATE INDEX ix_downloads_part ON downloads(part_id, quality);
