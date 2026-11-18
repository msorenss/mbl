-- Music requests (artist or album) go to Lidarr, which keys them by MusicBrainz ID.
-- tmdb_id is 0 for music requests.
ALTER TABLE media_requests ADD COLUMN musicbrainz_id TEXT;
ALTER TABLE media_requests ADD COLUMN artist TEXT;
