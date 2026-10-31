-- Online viewing is per-user and independent of local-library item ids.
CREATE TABLE online_watch (
 owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 source_id TEXT NOT NULL,
 document TEXT NOT NULL,
 position REAL NOT NULL,
 duration REAL NOT NULL,
 updated_at INTEGER NOT NULL,
 PRIMARY KEY(owner_id,source_id)
);
