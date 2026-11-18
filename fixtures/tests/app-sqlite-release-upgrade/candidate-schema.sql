CREATE TABLE IF NOT EXISTS oauth_state_uses (
                nonce TEXT PRIMARY KEY NOT NULL,
                expires_at INTEGER NOT NULL
            );
CREATE INDEX IF NOT EXISTS idx_oauth_state_uses_expiry ON oauth_state_uses(expires_at);
ALTER TABLE users ADD COLUMN session_epoch INTEGER NOT NULL DEFAULT 0;
CREATE TABLE IF NOT EXISTS client_grants (
                code_hash TEXT PRIMARY KEY NOT NULL,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                client TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                code_challenge TEXT NOT NULL,
                session_epoch INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
CREATE INDEX IF NOT EXISTS idx_client_grants_user ON client_grants(user_id);
CREATE INDEX IF NOT EXISTS idx_client_grants_expiry ON client_grants(expires_at);
CREATE TABLE IF NOT EXISTS client_sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                token_hash TEXT NOT NULL UNIQUE,
                client TEXT NOT NULL,
                session_epoch INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER,
                expires_at INTEGER NOT NULL
            );
CREATE INDEX IF NOT EXISTS idx_client_sessions_user ON client_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_client_sessions_expiry ON client_sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_client_sessions_token ON client_sessions(token_hash);
