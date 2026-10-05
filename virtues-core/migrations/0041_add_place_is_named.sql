-- 0041_add_place_is_named
--
-- Whether the person named this place. The resolver names a place it cannot
-- identify "Location <lat>, <lon>", and every reader that needed to tell those
-- apart from a real name matched on that text. A flag says it once: set when
-- the person renames a place or creates one, read by the naming queue and by
-- the rebuild that deletes empty places (a named place is never deleted).
--
-- Backfilled from what already told the two apart: a place made by the person
-- (metadata.source = 'user'), or any name the resolver would not have written.
ALTER TABLE wiki_places
    ADD COLUMN IF NOT EXISTS is_named boolean NOT NULL DEFAULT false;

UPDATE wiki_places
SET is_named = true
WHERE NOT is_named
  AND (metadata->>'source' = 'user' OR name NOT LIKE 'Location %');
