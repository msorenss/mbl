-- Migration 0008: Milestone M9 Live TV, Channels & Online Media Plugins

-- 1. Tuners & Lineup (M9.1)
CREATE TABLE tuners (
    id                  INTEGER PRIMARY KEY,
    name                TEXT NOT NULL,
    kind                TEXT NOT NULL, -- 'hdhomerun', 'm3u'
    url                 TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'online', -- 'online', 'offline', 'scanning'
    tuner_count         INTEGER NOT NULL DEFAULT 1,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);
CREATE INDEX ix_tuners_status ON tuners(status);

-- 2. Channels (M9.1, M9.2, M9.7)
CREATE TABLE channels (
    id                  INTEGER PRIMARY KEY,
    tuner_id            INTEGER REFERENCES tuners(id) ON DELETE SET NULL,
    channel_number      TEXT NOT NULL,
    name                TEXT NOT NULL,
    callsign            TEXT,
    logo_url            TEXT,
    stream_url          TEXT NOT NULL,
    epg_channel_id      TEXT,
    is_favorite         INTEGER NOT NULL DEFAULT 0,
    is_hidden           INTEGER NOT NULL DEFAULT 0,
    is_virtual          INTEGER NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);
CREATE INDEX ix_channels_number ON channels(channel_number);
CREATE INDEX ix_channels_epg ON channels(epg_channel_id);

-- 3. EPG Programs & Guide Grid (M9.1, M9.2)
CREATE TABLE epg_programs (
    id                  INTEGER PRIMARY KEY,
    channel_id          INTEGER REFERENCES channels(id) ON DELETE CASCADE,
    epg_channel_id      TEXT NOT NULL,
    title               TEXT NOT NULL,
    description         TEXT,
    season_number       INTEGER,
    episode_number      INTEGER,
    start_time          INTEGER NOT NULL,
    end_time            INTEGER NOT NULL,
    icon_url            TEXT,
    rating              TEXT,
    genres              TEXT NOT NULL DEFAULT '[]',
    is_new              INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX ix_epg_programs_time ON epg_programs(channel_id, start_time, end_time);
CREATE INDEX ix_epg_programs_epg ON epg_programs(epg_channel_id, start_time);

-- 4. Recording Rules & Series Rules (M9.4)
CREATE TABLE recording_rules (
    id                  INTEGER PRIMARY KEY,
    user_id             INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title               TEXT NOT NULL,
    channel_id          INTEGER REFERENCES channels(id) ON DELETE CASCADE,
    series_only         INTEGER NOT NULL DEFAULT 0,
    season_number       INTEGER,
    new_only            INTEGER NOT NULL DEFAULT 0,
    start_padding_secs  INTEGER NOT NULL DEFAULT 0,
    end_padding_secs    INTEGER NOT NULL DEFAULT 0,
    keep_max            INTEGER,
    auto_delete_watched INTEGER NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL
);
CREATE INDEX ix_recording_rules_user ON recording_rules(user_id);

-- 5. Recordings & Post-processing (M9.4)
CREATE TABLE recordings (
    id                  INTEGER PRIMARY KEY,
    rule_id             INTEGER REFERENCES recording_rules(id) ON DELETE SET NULL,
    channel_id          INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    program_id          INTEGER REFERENCES epg_programs(id) ON DELETE SET NULL,
    title               TEXT NOT NULL,
    start_time          INTEGER NOT NULL,
    end_time            INTEGER NOT NULL,
    status              TEXT NOT NULL DEFAULT 'scheduled', -- 'scheduled', 'recording', 'completed', 'failed', 'cancelled'
    output_path         TEXT,
    size_bytes          INTEGER,
    duration_ms         INTEGER,
    error_message       TEXT,
    created_at          INTEGER NOT NULL
);
CREATE INDEX ix_recordings_status ON recordings(status);
CREATE INDEX ix_recordings_time ON recordings(start_time, end_time);

-- 6. Online Media Plugins Configs (M9.6)
CREATE TABLE plugin_configs (
    id                  INTEGER PRIMARY KEY,
    plugin_id           TEXT NOT NULL UNIQUE,
    name                TEXT NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 1,
    settings            TEXT NOT NULL DEFAULT '{}',
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);

-- 7. Virtual 24/7 Continuous Channels (M9.7)
CREATE TABLE virtual_channels (
    id                  INTEGER PRIMARY KEY,
    channel_id          INTEGER NOT NULL UNIQUE REFERENCES channels(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    mode                TEXT NOT NULL DEFAULT 'linear_loop', -- 'linear_loop', 'shuffle', 'smart_filter'
    items               TEXT NOT NULL DEFAULT '[]',
    insert_commercials  INTEGER NOT NULL DEFAULT 0,
    total_duration_ms   INTEGER NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL
);
