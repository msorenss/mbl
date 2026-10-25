-- Hardware acceleration functional self-test results (M4.3).
CREATE TABLE hw_capabilities (
    backend            TEXT NOT NULL,
    codec              TEXT NOT NULL,
    operation          TEXT NOT NULL,
    ok                 INTEGER NOT NULL DEFAULT 0,
    fps                REAL,
    error              TEXT,
    ffmpeg_hash        TEXT NOT NULL,
    driver_fingerprint TEXT NOT NULL,
    tested_at          INTEGER NOT NULL,
    PRIMARY KEY (backend, codec, operation)
);
