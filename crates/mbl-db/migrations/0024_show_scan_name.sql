-- The name the scanner read from the files for a show. Metadata may rename the show
-- ("The Rings of Power" -> "The Lord of the Rings: The Rings of Power"); new files are still
-- matched to it by this name. Existing shows start with their current title.
ALTER TABLE media_items ADD COLUMN scan_name TEXT;
UPDATE media_items SET scan_name = title WHERE kind = 'show' AND parent_id IS NULL;
