-- Milestone M6: Users, sharing, remote access, TOTP 2FA, invites, library ACLs, and MCP OAuth 2.1

-- User enhancements: managed users, PIN hash, content rating ceiling, TOTP 2FA
ALTER TABLE users ADD COLUMN is_managed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN pin_hash TEXT;
ALTER TABLE users ADD COLUMN content_rating_ceiling TEXT;
ALTER TABLE users ADD COLUMN totp_secret TEXT;
ALTER TABLE users ADD COLUMN totp_enabled INTEGER NOT NULL DEFAULT 0;

-- Per-user library access grants
CREATE TABLE user_library_access (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    library_id  INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (user_id, library_id)
);
CREATE INDEX ix_user_lib_access_user ON user_library_access(user_id);

-- Invites: expiring share links with optional library scope and content rating ceilings
CREATE TABLE invites (
    id                      INTEGER PRIMARY KEY,
    code                    TEXT NOT NULL UNIQUE,
    created_by              INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    library_ids             TEXT NOT NULL DEFAULT '[]',
    content_rating_ceiling  TEXT,
    expires_at              INTEGER NOT NULL,
    used_at                 INTEGER,
    used_by                 INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at              INTEGER NOT NULL
);
CREATE INDEX ix_invites_code ON invites(code);

-- Remote access & networking configuration
CREATE TABLE remote_settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- OAuth 2.1 for MCP (M6.17)
CREATE TABLE oauth_clients (
    client_id               TEXT PRIMARY KEY,
    client_name             TEXT NOT NULL,
    redirect_uris           TEXT NOT NULL,
    grant_types             TEXT NOT NULL DEFAULT '["authorization_code","refresh_token"]',
    response_types          TEXT NOT NULL DEFAULT '["code"]',
    token_endpoint_auth     TEXT NOT NULL DEFAULT 'none',
    created_at              INTEGER NOT NULL
);

CREATE TABLE oauth_grants (
    id                      INTEGER PRIMARY KEY,
    code                    TEXT UNIQUE,
    code_challenge          TEXT,
    code_challenge_method   TEXT,
    client_id               TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
    user_id                 INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scopes                  TEXT NOT NULL,
    redirect_uri            TEXT NOT NULL,
    access_token_hash       BLOB UNIQUE,
    refresh_token_hash      BLOB UNIQUE,
    code_expires_at         INTEGER,
    access_token_expires_at INTEGER,
    revoked_at              INTEGER,
    created_at              INTEGER NOT NULL
);
CREATE INDEX ix_oauth_grants_code ON oauth_grants(code);
CREATE INDEX ix_oauth_grants_access_token ON oauth_grants(access_token_hash);
CREATE INDEX ix_oauth_grants_refresh_token ON oauth_grants(refresh_token_hash);
