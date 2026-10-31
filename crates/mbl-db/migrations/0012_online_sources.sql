-- Derived per-user metadata: public hosts may still carry private signed URLs.
CREATE TABLE online_sources (
    owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    url TEXT NOT NULL,
    document TEXT NOT NULL,
    inspected_at INTEGER NOT NULL,
    PRIMARY KEY(owner_id, id),
    UNIQUE(owner_id, url)
);
CREATE INDEX ix_online_sources_checked ON online_sources(inspected_at DESC, owner_id, id);
