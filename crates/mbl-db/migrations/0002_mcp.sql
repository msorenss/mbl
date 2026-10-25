CREATE TABLE api_tokens (
  id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL, token_hash BLOB NOT NULL UNIQUE, scopes TEXT NOT NULL,
  allowed_networks TEXT NOT NULL DEFAULT '["lan","tailnet"]',
  created_at INTEGER NOT NULL, expires_at INTEGER, last_used_at INTEGER, revoked_at INTEGER);

CREATE TABLE mcp_audit (
  id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, user_id INTEGER, token_id INTEGER, client TEXT,
  tool TEXT NOT NULL, args TEXT NOT NULL, outcome TEXT NOT NULL, error TEXT,
  duration_ms INTEGER NOT NULL, dry_run INTEGER NOT NULL DEFAULT 0);
CREATE INDEX ix_mcp_audit_ts ON mcp_audit(ts DESC);
CREATE INDEX ix_mcp_audit_token ON mcp_audit(token_id, ts DESC);
