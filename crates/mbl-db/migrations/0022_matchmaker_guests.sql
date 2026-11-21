-- Guests at a movie night: people without an account who join through the host's link. A guest's
-- votes are bound to a token handed out when they join, never to the name they type.
ALTER TABLE matchmaker_sessions ADD COLUMN guest_code TEXT;
CREATE UNIQUE INDEX matchmaker_sessions_guest_code
    ON matchmaker_sessions(guest_code) WHERE guest_code IS NOT NULL;

CREATE TABLE matchmaker_guests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES matchmaker_sessions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash BLOB NOT NULL UNIQUE,
    joined_at INTEGER NOT NULL
);
CREATE INDEX matchmaker_guests_session ON matchmaker_guests(session_id);
