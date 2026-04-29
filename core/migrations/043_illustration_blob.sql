-- Migration 043: Add illustration BLOB to wiki_days
--
-- Stores the day's pen-and-ink illustration as a transparent PNG directly
-- in the day row. ~100-400KB per image. Served via GET /api/wiki/day/:date/illustration.
-- Eliminates filesystem management — illustration is a property of the day,
-- not a file in a directory.

ALTER TABLE wiki_days ADD COLUMN illustration BLOB;
