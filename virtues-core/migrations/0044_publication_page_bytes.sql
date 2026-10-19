-- 0044_publication_page_bytes
--
-- The frozen page itself, in the database. Backups are the database dump
-- plus the lake, so a page that lived only in the door's bundle directory
-- came back from a restore as a link that opens nothing. The core rewrites
-- the bundle directory from these rows when it starts the door
-- (crate::door), which makes the directory a copy of this column. The door
-- key moves into box_secrets for the same reason.
--
-- NULL on rows made before this migration: those bundles stay as they are on
-- disk and are not rebuilt.

ALTER TABLE app_publications ADD COLUMN IF NOT EXISTS page BYTEA;
