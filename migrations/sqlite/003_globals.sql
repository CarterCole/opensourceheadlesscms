-- Site-wide globals stored in the database backend (`[globals] storage = "database"`).
-- `value` is the JSON-encoded value; definitions live in YAML, not here.
CREATE TABLE IF NOT EXISTS _cms_globals (
    name       TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT NOT NULL
);
