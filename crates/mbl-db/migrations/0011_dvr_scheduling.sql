-- Keep archive files independent of tuner/channel lifetime, and never reuse a
-- recording id while an old cancelled FFmpeg child may still be cleaning up.
CREATE TABLE recordings_next (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id INTEGER REFERENCES recording_rules(id) ON DELETE SET NULL,
    channel_id INTEGER REFERENCES channels(id) ON DELETE SET NULL,
    program_id INTEGER REFERENCES epg_programs(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    start_time INTEGER NOT NULL,
    end_time INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'scheduled',
    output_path TEXT,
    size_bytes INTEGER,
    duration_ms INTEGER,
    error_message TEXT,
    created_at INTEGER NOT NULL,
    item_id INTEGER REFERENCES media_items(id) ON DELETE SET NULL,
    content_rating TEXT,
    summary TEXT,
    owner_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    library_id INTEGER REFERENCES libraries(id) ON DELETE SET NULL,
    program_start_time INTEGER NOT NULL,
    program_end_time INTEGER NOT NULL,
    recorded_channel_name TEXT,
    deleted_at INTEGER,
    suppress_auto INTEGER NOT NULL DEFAULT 0
);
INSERT INTO recordings_next
SELECT r.id, r.rule_id, r.channel_id, r.program_id, r.title, r.start_time,
    r.end_time, r.status, r.output_path, r.size_bytes, r.duration_ms,
    r.error_message, r.created_at, r.item_id, r.content_rating, r.summary,
    COALESCE((SELECT user_id FROM recording_rules WHERE id=r.rule_id),
             (SELECT id FROM users WHERE is_admin=1 ORDER BY id LIMIT 1)),
    (SELECT library_id FROM media_items WHERE id=r.item_id),
    COALESCE((SELECT start_time FROM epg_programs WHERE id=r.program_id),r.start_time),
    COALESCE((SELECT end_time FROM epg_programs WHERE id=r.program_id),r.end_time),
    (SELECT name FROM channels WHERE id=r.channel_id), NULL, 0
FROM recordings r;
DROP TABLE recordings;
ALTER TABLE recordings_next RENAME TO recordings;
CREATE INDEX ix_recordings_status ON recordings(status) WHERE deleted_at IS NULL;
CREATE INDEX ix_recordings_time ON recordings(start_time,end_time);
CREATE UNIQUE INDEX ix_recordings_item ON recordings(item_id) WHERE item_id IS NOT NULL;
CREATE INDEX ix_recordings_occurrence ON recordings(owner_id,channel_id,program_start_time);
CREATE TRIGGER dvr_channel_removed BEFORE DELETE ON channels BEGIN
    UPDATE recordings SET status='failed',error_message='Source channel was removed'
    WHERE channel_id=OLD.id AND deleted_at IS NULL AND status IN ('scheduled','recording');
END;
CREATE TRIGGER dvr_owner_removed BEFORE DELETE ON users BEGIN
    UPDATE recordings SET status='failed',error_message='Recording owner was removed'
    WHERE owner_id=OLD.id AND deleted_at IS NULL AND status IN ('scheduled','recording');
END;

-- Re-importing the same XMLTV occurrence updates it instead of duplicating it.
DELETE FROM epg_programs WHERE id NOT IN (
    SELECT MAX(id) FROM epg_programs GROUP BY epg_channel_id,start_time
);
CREATE UNIQUE INDEX ix_epg_occurrence ON epg_programs(epg_channel_id,start_time);
