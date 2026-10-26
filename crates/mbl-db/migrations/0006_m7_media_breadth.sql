-- Milestone M7: Media breadth (Music, Photos, Collections, Playlists, Metadata locks, Webhooks, Media Requests, Matchmaker)

-- 1. Music & Photo metadata columns in media_items
ALTER TABLE media_items ADD COLUMN musicbrainz_id TEXT;
ALTER TABLE media_items ADD COLUMN disc_number INTEGER;
ALTER TABLE media_items ADD COLUMN track_number INTEGER;
ALTER TABLE media_items ADD COLUMN replaygain_track_gain REAL;
ALTER TABLE media_items ADD COLUMN replaygain_track_peak REAL;
ALTER TABLE media_items ADD COLUMN replaygain_album_gain REAL;
ALTER TABLE media_items ADD COLUMN replaygain_album_peak REAL;
ALTER TABLE media_items ADD COLUMN loudness_lufs REAL;
ALTER TABLE media_items ADD COLUMN loudness_range REAL;
ALTER TABLE media_items ADD COLUMN true_peak REAL;
ALTER TABLE media_items ADD COLUMN lyrics TEXT;

-- Photos EXIF columns in media_items
ALTER TABLE media_items ADD COLUMN exif_date INTEGER;
ALTER TABLE media_items ADD COLUMN camera_make TEXT;
ALTER TABLE media_items ADD COLUMN camera_model TEXT;
ALTER TABLE media_items ADD COLUMN latitude REAL;
ALTER TABLE media_items ADD COLUMN longitude REAL;

-- Subsonic password/salt support in users
ALTER TABLE users ADD COLUMN subsonic_salt TEXT;
ALTER TABLE users ADD COLUMN subsonic_token TEXT;

-- 2. Collections (M7.6)
CREATE TABLE collections (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    overview     TEXT,
    poster_id    INTEGER,
    backdrop_id  INTEGER,
    is_smart     INTEGER NOT NULL DEFAULT 0,
    smart_filter TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);
CREATE INDEX ix_collections_name ON collections(name);

CREATE TABLE collection_items (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    item_id       INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, item_id)
);
CREATE INDEX ix_collection_items_coll ON collection_items(collection_id, sort_order);

-- 3. Playlists (M7.6)
CREATE TABLE playlists (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    is_public    INTEGER NOT NULL DEFAULT 0,
    is_smart     INTEGER NOT NULL DEFAULT 0,
    smart_filter TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);
CREATE INDEX ix_playlists_user ON playlists(user_id);

CREATE TABLE playlist_items (
    id          INTEGER PRIMARY KEY,
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    item_id     INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX ix_playlist_items_pl ON playlist_items(playlist_id, sort_order);

-- 4. Item metadata locks (M7.7)
CREATE TABLE item_metadata_locks (
    item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    field   TEXT NOT NULL,
    PRIMARY KEY (item_id, field)
);

-- 5. Webhooks (M7.11)
CREATE TABLE webhooks (
    id         INTEGER PRIMARY KEY,
    url        TEXT NOT NULL,
    name       TEXT NOT NULL,
    events     TEXT NOT NULL,
    secret     TEXT,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);

-- 6. Media Requests & Sonarr/Radarr Bridge (M7.20)
CREATE TABLE media_requests (
    id         INTEGER PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    tmdb_id    INTEGER NOT NULL,
    title      TEXT NOT NULL,
    year       INTEGER,
    status     TEXT NOT NULL DEFAULT 'pending',
    arr_id     INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX ix_media_requests_user ON media_requests(user_id);
CREATE INDEX ix_media_requests_status ON media_requests(status);

-- 7. Movie Night Matchmaker (M7.19)
CREATE TABLE matchmaker_sessions (
    id              TEXT PRIMARY KEY,
    created_by      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    filter_config   TEXT NOT NULL DEFAULT '{}',
    status          TEXT NOT NULL DEFAULT 'active',
    matched_item_id INTEGER REFERENCES media_items(id) ON DELETE SET NULL,
    created_at      INTEGER NOT NULL
);

CREATE TABLE matchmaker_votes (
    session_id TEXT NOT NULL REFERENCES matchmaker_sessions(id) ON DELETE CASCADE,
    user_name  TEXT NOT NULL,
    item_id    INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    vote       INTEGER NOT NULL,
    PRIMARY KEY (session_id, user_name, item_id)
);
