-- 0080 — The self person: which node in the graph is the owner.
--
-- The wiki had no self node, so every relationship was one-sided.
-- `wiki_people.relationship_category` says "sister" without ever saying *to
-- whom*, and "who is my family" had nowhere to live. Worse, the owner already
-- appears in the graph as ordinary people rows — a real box carries both
-- "Adam Jace" (205 refs) and "adam" (25 refs), minted from contacts and email
-- like anyone else — so the record contains the user without knowing it does.
--
-- The pointer lives on the PROFILE, not as an `is_self` flag on wiki_people.
-- Three reasons:
--
--   1. `app_user_profile` is already the singleton that answers "who owns this
--      appliance", and it already soft-references the graph exactly this way:
--      `home_place_id` points into wiki_places with no FK (0003). This is the
--      same edge, drawn in the same direction, by the same table.
--   2. Singularity is then structural. One column on a one-row table cannot
--      hold two selves; an `is_self` flag on the many-side needs a partial
--      unique index to say the same thing, and can be violated in between.
--   3. It settles ownership, which was ambiguous. The profile owns the
--      IDENTITY — full_name, preferred_name, birth_date — and is already
--      injected into every chat prompt by build_user_context(). The person row
--      owns the GRAPH POSITION: the anchor relationships point at, and the
--      subject of the owner's own messages. Neither restates the other, and
--      the direction of the pointer is what says which is which.
--
-- No FK, deliberately, matching `home_place_id`: the wiki tables are rebuilt by
-- resolution passes, and a hard FK from the profile would make an ordinary
-- entity cleanup fail against the one row on the box that must never fail.
-- A dangling id reads as "not set", which is also the correct state before
-- anyone has chosen.

ALTER TABLE app_user_profile ADD COLUMN self_person_id TEXT;

COMMENT ON COLUMN app_user_profile.self_person_id IS
    'Soft reference into wiki_people: which graph node is the owner. No FK, matching home_place_id.';

-- Best-effort backfill under the 0037 rule: link only when the surface matches
-- EXACTLY ONE person. Two candidates means the resolver declines and a human
-- chooses later — the same discipline aliases use, for the same reason. Getting
-- this wrong would attribute the owner's entire message history to a stranger,
-- so ambiguity must not be allowed to resolve itself.
UPDATE app_user_profile p
SET self_person_id = (
    SELECT w.id FROM wiki_people w
    WHERE lower(trim(w.canonical_name)) = lower(trim(p.full_name))
)
WHERE p.self_person_id IS NULL
  AND p.full_name IS NOT NULL
  AND trim(p.full_name) <> ''
  AND (
    SELECT count(*) FROM wiki_people w
    WHERE lower(trim(w.canonical_name)) = lower(trim(p.full_name))
  ) = 1;
