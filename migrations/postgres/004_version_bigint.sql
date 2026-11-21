-- `version` was INTEGER (INT4), but the store reads it as i64 (INT8), and sqlx refuses to decode
-- INT4 into i64: every read of a content item failed on PostgreSQL ("mismatched types ... INT4").
-- SQLite has no such distinction. Widening the columns matches the code everywhere it reads them.
ALTER TABLE _cms_content ALTER COLUMN version TYPE BIGINT;
ALTER TABLE _cms_content_versions ALTER COLUMN version TYPE BIGINT;
