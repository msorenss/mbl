-- A movie night can be planned for a time: voting closes then and the most liked title wins.
ALTER TABLE matchmaker_sessions ADD COLUMN starts_at INTEGER;
CREATE INDEX matchmaker_sessions_due ON matchmaker_sessions(status, starts_at)
    WHERE starts_at IS NOT NULL;
