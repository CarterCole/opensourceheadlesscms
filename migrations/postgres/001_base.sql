CREATE TABLE IF NOT EXISTS _cms_content (
    id           TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'draft',
    fields       TEXT NOT NULL DEFAULT '{}',
    version      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    created_by   TEXT NOT NULL,
    scheduled_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_content_type_status ON _cms_content(content_type, status);

CREATE TABLE IF NOT EXISTS _cms_content_versions (
    id           TEXT PRIMARY KEY,
    content_id   TEXT NOT NULL,
    content_type TEXT NOT NULL,
    version      INTEGER NOT NULL,
    fields       TEXT NOT NULL,
    status       TEXT NOT NULL,
    created_by   TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    FOREIGN KEY (content_id) REFERENCES _cms_content(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_versions_content ON _cms_content_versions(content_id, version DESC);

CREATE TABLE IF NOT EXISTS _cms_users (
    id            TEXT PRIMARY KEY,
    email         TEXT UNIQUE NOT NULL,
    display_name  TEXT NOT NULL,
    bio           TEXT,
    avatar_url    TEXT,
    password_hash TEXT NOT NULL,
    roles         TEXT NOT NULL DEFAULT '[]',
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS _cms_sessions (
    token      TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES _cms_users(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS _cms_api_tokens (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    token_hash TEXT UNIQUE NOT NULL,
    user_id    TEXT NOT NULL,
    scopes     TEXT NOT NULL DEFAULT '[]',
    expires_at TEXT,
    last_used  TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES _cms_users(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS _cms_media (
    id           TEXT PRIMARY KEY,
    filename     TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size         BIGINT NOT NULL,
    url          TEXT NOT NULL,
    uploaded_by  TEXT NOT NULL,
    created_at   TEXT NOT NULL
);

-- Full-text search index (replaces SQLite FTS5)
CREATE TABLE IF NOT EXISTS _cms_search_index (
    id           TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    text_blob    TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_search_fts
    ON _cms_search_index USING GIN (to_tsvector('english', text_blob));

CREATE TABLE IF NOT EXISTS _cms_migrations (
    name       TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL
);
