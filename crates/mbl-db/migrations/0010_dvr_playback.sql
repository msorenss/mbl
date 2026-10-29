ALTER TABLE recordings ADD COLUMN item_id INTEGER REFERENCES media_items(id) ON DELETE SET NULL;
ALTER TABLE recordings ADD COLUMN content_rating TEXT;
ALTER TABLE recordings ADD COLUMN summary TEXT;
UPDATE recordings SET content_rating=(SELECT rating FROM epg_programs WHERE id=recordings.program_id),
    summary=(SELECT description FROM epg_programs WHERE id=recordings.program_id);
-- Earlier versions stored wall-clock job time here, which is not media duration.
UPDATE recordings SET duration_ms=NULL;
CREATE UNIQUE INDEX ix_recordings_item ON recordings(item_id) WHERE item_id IS NOT NULL;

-- The recorder owns this library's contents; general scanning must not import partial files.
CREATE TABLE dvr_libraries (
    library_id INTEGER PRIMARY KEY REFERENCES libraries(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE
);
