-- Episodes and movies whose intro/credits analysis has run for their current file, so the
-- background job does not repeat it after every scan. A replaced file gets a new part id.
CREATE TABLE marker_analysis (
    item_id     INTEGER PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    part_id     INTEGER NOT NULL REFERENCES media_parts(id) ON DELETE CASCADE,
    analyzed_at INTEGER NOT NULL
);
