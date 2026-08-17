-- ---------------------------------------------------------------------------
-- wiki_entity_refs → wiki_refs: a ref points at a SUBJECT, not only an entity.
--
-- The table has always been the general citation edge — "record R mentions
-- subject S at time T" — but its name and columns admitted only people, places,
-- organizations and things. That narrowness is what makes the attention
-- question unanswerable today: "how often did the record return to THIS
-- EVENING?" has to be approximated as "how often did a resolved PERSON from
-- that evening appear elsewhere", which is a different question and a much
-- worse one. It saturates on high-contact people (a family thread outranks a
-- singular occasion) and scores zero on exactly the events that matter most —
-- a first meeting with someone who has no `wiki_people` row yet.
--
-- Widening the subject fixes both. Three new types:
--
--   'event'  — a wiki_events row. Lets a record cite a stretch of a day
--              directly, so "the record returned to this evening" becomes a
--              COUNT over this table instead of an inference.
--   'day'    — a wiki_days row, for records that refer to a date as a whole.
--   'thread' — a data_communication_message.thread_id. THE important one: a
--              conversation has an identity from its first message, months
--              before the person on the other end is ever resolved into the
--              graph. Anchoring on the thread is what lets a brand-new
--              correspondent register at all.
--
-- This migration is a pure rename plus a widened CHECK. No rows move, no rows
-- are written, and nothing yet writes the three new types — the deterministic
-- resolvers (entity_resolution::{people,places}) keep emitting exactly what
-- they emitted before. Producing event/day/thread refs is separate work with
-- its own doctrine question, and deliberately not smuggled in here.
--
-- NOTE the pre-existing unique index (renamed below, unchanged in definition):
-- (entity_id, source_table, source_id, role) NULLS NOT DISTINCT. Refs cannot
-- duplicate. Any future automated writer gets idempotency from this index for
-- free — which is precisely what `wiki_notes` lacks.
--
-- THE COLUMNS KEEP THEIR NAMES, deliberately. `entity_id`/`entity_type` would
-- read better as `subject_id`/`subject_type` now that the subject can be a
-- moment rather than a noun — but that identifier is overloaded across three
-- unrelated concepts in this codebase: this table, the ref-ROUTE addressing
-- scheme (`/person/person_ab12` — RefPicker, editAllowList, chat_permissions,
-- the CodeMirror ref extension), and the dead `er_mentions` from the semantic
-- ER that 0061 removed. 166 occurrences across 27 files, and a mechanical
-- rename breaks the ref picker. The capability is the widened CHECK; the
-- column name is cosmetics, and cosmetics do not get to carry that risk.
-- ---------------------------------------------------------------------------

ALTER TABLE wiki_entity_refs RENAME TO wiki_refs;

-- The inline column CHECK from 0006 carries Postgres's generated name, which
-- survives the table rename. Drop it by that name and re-add the widened one
-- under a name that matches where we ended up.
ALTER TABLE wiki_refs DROP CONSTRAINT IF EXISTS wiki_entity_refs_entity_type_check;
ALTER TABLE wiki_refs ADD  CONSTRAINT wiki_refs_subject_type_check
    CHECK (entity_type IN (
        -- entities: written today by the deterministic resolvers
        'person', 'place', 'organization', 'thing',
        -- wiki objects and conversations: no writers yet, see header
        'event', 'day', 'thread'
    ));

ALTER INDEX idx_entity_refs_entity      RENAME TO idx_wiki_refs_subject;
ALTER INDEX idx_entity_refs_source      RENAME TO idx_wiki_refs_source;
ALTER INDEX idx_entity_refs_type        RENAME TO idx_wiki_refs_subject_type;
ALTER INDEX idx_entity_refs_source_type RENAME TO idx_wiki_refs_source_subject_type;
ALTER INDEX idx_entity_refs_unique      RENAME TO idx_wiki_refs_unique;

COMMENT ON TABLE wiki_refs IS
    'The citation edge: source record R refers to subject S at time T. Subjects '
    'are entities (person/place/organization/thing) or wiki objects and '
    'conversations (event/day/thread). Written deterministically — semantic '
    'extraction into this table was removed in 0061 and is not coming back.';

COMMENT ON COLUMN wiki_refs.entity_type IS
    'What kind of thing is being referred to — the SUBJECT type, despite the '
    'column name (see 0099 header on why the rename was not worth its blast '
    'radius). Widening this is how a ref became able to cite a moment rather '
    'than only a noun.';
