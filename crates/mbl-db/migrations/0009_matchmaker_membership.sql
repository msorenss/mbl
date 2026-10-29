CREATE TABLE matchmaker_members (
    session_id TEXT NOT NULL REFERENCES matchmaker_sessions(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (session_id, user_id)
);
INSERT INTO matchmaker_members (session_id, user_id, joined_at)
SELECT id, created_by, created_at FROM matchmaker_sessions;
-- Prior votes used freely supplied display names, so cannot count as authenticated votes.
DELETE FROM matchmaker_votes;
