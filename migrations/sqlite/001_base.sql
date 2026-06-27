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
CREATE INDEX IF NOT EXISTS idx_content_slug ON _cms_content(content_type, json_extract(fields, '$.slug'));

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
    id           TEXT PRIMARY KEY,
    email        TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    bio          TEXT,
    avatar_url   TEXT,
    password_hash TEXT NOT NULL,
    roles        TEXT NOT NULL DEFAULT '[]',
    created_at   TEXT NOT NULL
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
    size         INTEGER NOT NULL,
    url          TEXT NOT NULL,
    uploaded_by  TEXT NOT NULL,
    created_at   TEXT NOT NULL
);

-- FTS5 for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS _cms_search_fts USING fts5(
    id UNINDEXED,
    content_type UNINDEXED,
    text_blob,
    tokenize = 'porter unicode61'
);

CREATE TABLE IF NOT EXISTS _cms_migrations (
    name       TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL
);
