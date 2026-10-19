-- `wiki_refs.entity_type` says what a record can POINT AT, and it has drifted.
--
-- A ref is a citation: "this email mentions Maya", "this visit happened at the
-- office". What it points at is an ENTITY — the output of entity resolution —
-- and there are exactly three kinds, which is what makes this list different
-- from the subject_type lists migration 0022 aligned. A record cannot
-- reference a year: a year is a partition we impose, not something a message
-- mentions. That is why 0022 deliberately left this column alone, and why
-- widening it to match `wiki_articles.subject_type` would be wrong.
--
-- What it allowed until now:
--
--     person, place, organization, thing, event, day, thread
--
-- Four of those cannot be produced and cannot be read:
--
--   * `thing` — `wiki_things` was dropped by the migration that swept every
--     stored `/thing/` url. Nothing can hold a thing to point at.
--   * `event`, `day`, `thread` — no writer has ever emitted one. Every
--     `INSERT INTO wiki_refs` in the tree (entity_resolution/people.rs,
--     entity_resolution/places.rs, and the reclassify path in api/entities.rs)
--     writes one of the three literals, and nothing anywhere does
--     `SET entity_type`.
--
-- And nothing READS them: every query that filters this column names 'person',
-- 'place' or 'organization' — the day dossier, the sql_query join hints, the
-- circumstances lookups, the place streams. A row carrying one of the four is
-- invisible to the whole system, which is precisely why it could sit in the
-- constraint for months without anyone noticing.
--
-- The DELETE runs FIRST and unconditionally. A narrowed CHECK is validated
-- against existing rows, so a box holding one legacy row would fail the ALTER,
-- fail the migration, and refuse to boot — the failure mode that took a box
-- down for three and a quarter hours once already. Rows removed here are
-- unreachable by every reader and point at no entity table, so nothing that
-- could be displayed, searched or cited is lost.

DELETE FROM wiki_refs
 WHERE entity_type NOT IN ('person', 'place', 'organization');

ALTER TABLE wiki_refs DROP CONSTRAINT IF EXISTS wiki_refs_entity_type_check;
ALTER TABLE wiki_refs ADD CONSTRAINT wiki_refs_entity_type_check
    CHECK (entity_type = ANY (ARRAY[
        'person'::text, 'place'::text, 'organization'::text
    ]));

-- The three entity-shaped subjects, and only those. `agents/build/glossary.md`
-- is where the three shapes a subject can take are defined, and
-- `virtues-core/src/api/subjects.rs` is that table in code.
