-- ---------------------------------------------------------------------------
-- The identifiers a person answers to (docs/mac-presence-plan.md follow-on)
--
-- The box knows 525 people. It holds thousands of iMessages. NONE of them were
-- connected: every message said `+15125550100` and not one said "Nick".
--
-- The data was all there. The join was impossible, because the two sides spell the
-- same person differently:
--
--     iOS Contacts →  "(512) 555-0142"     stored RAW, exactly as typed
--     chat.db      →  "+15125550142"       E.164
--
-- As strings those never match. And the only matcher that existed was a
-- `LIKE '%digits%'` substring scan over the `phones` JSONB — unindexed (a seq scan
-- per lookup) and *wrong*: a 7-digit number matches inside a different country's
-- 11-digit number, which silently attributes messages to the wrong human. There
-- was, separately, no resolver for data_communication_message at all — email
-- senders resolved, message senders were never wired up.
--
-- `handles` is the normal form: E.164 phones and lowercased emails, computed once
-- on write. Resolution becomes a single containment check against a GIN index.
--
-- A JSONB array on the entity rather than a handles table, following 0037's
-- reasoning verbatim: wiki_people already carries `emails`, `phones` and `aliases`
-- this way, and a person answering to two handles is not a constraint to enforce,
-- it is just a fact to store. `emails`/`phones` stay as they are — they are what
-- the human typed, and a normal form is not a replacement for the original.
--
-- Short codes get no handle at all (see virtues_helpers::handles): `22395` is a
-- bank's 2FA robot, not a person, and it must never resolve to one.
-- ---------------------------------------------------------------------------

ALTER TABLE wiki_people ADD COLUMN handles JSONB NOT NULL DEFAULT '[]'::jsonb;

-- `handles ? '+15125550142'` is the resolver's hot path — once per distinct sender.
-- GIN with jsonb_ops supports the existence operator directly, same as 0037's
-- aliases index.
CREATE INDEX idx_wiki_people_handles ON wiki_people USING GIN (handles);

-- Backfill from what's already stored. The phone rules here mirror
-- virtues_helpers::handles::normalize_phone: strip to digits, +1 a bare NANP
-- number, keep an explicit country code, and drop anything under 7 digits.
UPDATE wiki_people p
SET handles = COALESCE(
    (
        SELECT jsonb_agg(DISTINCT h)
        FROM (
            SELECT lower(e) AS h
            FROM jsonb_array_elements_text(p.emails) AS e
            WHERE e <> ''

            UNION

            SELECT CASE
                     WHEN raw LIKE '+%'          THEN '+' || digits
                     WHEN length(digits) = 11
                          AND left(digits,1) = '1' THEN '+' || digits
                     WHEN length(digits) = 10      THEN '+1' || digits
                     ELSE '+' || digits
                   END AS h
            FROM (
                SELECT ph AS raw,
                       regexp_replace(ph, '[^0-9]', '', 'g') AS digits
                FROM jsonb_array_elements_text(p.phones) AS ph
            ) x
            WHERE length(digits) >= 7   -- shorter is a short code, not a person
        ) handles_of_person
    ),
    '[]'::jsonb
);
