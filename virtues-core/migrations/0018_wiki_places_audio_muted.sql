-- 0018_wiki_places_audio_muted
--
-- A place can ask the phone not to keep audio while the owner is inside it.
-- The flag lives on the place row rather than as a second list of circles on
-- the phone: the wiki already knows the person's places and their radius, and
-- the phone only caches the muted subset. A column rather than a metadata key
-- because this schema is shown to a model at runtime and `is_` booleans are the
-- convention it matches; IF NOT EXISTS so a dev database that had the column
-- applied by hand for sqlx's compile-time check still boots.
ALTER TABLE wiki_places
    ADD COLUMN IF NOT EXISTS is_audio_muted boolean NOT NULL DEFAULT false;
