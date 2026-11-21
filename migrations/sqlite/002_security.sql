-- Record who last edited an item. `created_by` is now immutable (original author).
ALTER TABLE _cms_content ADD COLUMN updated_by TEXT NOT NULL DEFAULT '';
-- Best effort for existing rows: before this migration `created_by` held the last editor.
UPDATE _cms_content SET updated_by = created_by WHERE updated_by = '';

-- Bumping this invalidates every session token issued to the user (logout, revocation).
ALTER TABLE _cms_users ADD COLUMN session_epoch INTEGER NOT NULL DEFAULT 0;
