-- 0082 — Two note systems, two names: app_marginalia and wiki_notes.
--
-- There were two tables for "a note in a margin", and they were named the wrong
-- way round. `app_annotations` (0057) holds highlights and margin notes on a
-- document you are reading — the literal thing the word marginalia describes.
-- `wiki_marginalia` (0033) holds editorial notes about a subject in the record,
-- which is a plainer idea and wants a plainer word.
--
-- An earlier draft unified them. That was wrong: they differ in anchor (a page
-- passage vs a subject), in author (you vs you-or-the-machine), in whether a
-- citation is required, and in lifecycle (a highlight is never "resolved"; an
-- editorial note is accepted, dismissed, or absorbed). The unification argument
-- was that wiki notes would eventually need quote anchoring and would rebuild
-- what 0057 already solved — splitting them retires that argument instead of
-- answering it, because wiki notes are subject-scoped and never get anchors.
--
-- Doing this now costs nothing and will never be cheaper: `app_annotations` has
-- 1 row on a real box and `wiki_marginalia` has 0, with zero producers. A
-- catalog rename is instant and does not rewrite either table.
--
-- The vocabulary that falls out: in a document, the rail is the MARGINALIA and
-- a marked passage is a HIGHLIGHT; in the wiki, the rail is NOTES. The unit in
-- both is a NOTE, which has a singular that "marginalia" does not. "Annotation"
-- retires from the schema entirely.

-- ---------------------------------------------------------------------------
-- wiki_marginalia → wiki_notes
-- ---------------------------------------------------------------------------
-- ALTER TABLE ... RENAME renames the TABLE and nothing else: its indexes,
-- CHECK constraints and identity sequence keep their old names. Leaving them is
-- not cosmetic — the CHECK widen below has to name the constraint, and naming
-- it `wiki_notes_subject_type_check` before renaming it would fail and abort
-- the migration mid-upgrade. 0033 set the precedent by renaming four indexes
-- after the notebook rename; this does the same for every artifact.
ALTER TABLE wiki_marginalia RENAME TO wiki_notes;

ALTER INDEX idx_wiki_marginalia_subject RENAME TO idx_wiki_notes_subject;
ALTER INDEX wiki_marginalia_pkey        RENAME TO wiki_notes_pkey;
ALTER SEQUENCE wiki_marginalia_id_seq   RENAME TO wiki_notes_id_seq;

ALTER TABLE wiki_notes RENAME CONSTRAINT wiki_marginalia_subject_type_check
                                      TO wiki_notes_subject_type_check;
ALTER TABLE wiki_notes RENAME CONSTRAINT wiki_marginalia_kind_check
                                      TO wiki_notes_kind_check;
ALTER TABLE wiki_notes RENAME CONSTRAINT wiki_marginalia_author_check
                                      TO wiki_notes_author_check;

-- Now the CHECK can be widened by its real name.
--
-- Gains 'narrative_identity' (the propose-only channel writes here). Drops
-- 'telos': `wiki_telos` is the structural parent of acts and chapters — which
-- era a day belongs to — not a values document, and letting the note table
-- accept it invites exactly the merge the design forbids. Uses 'organization',
-- not the old 'org': `wiki_entity_refs.entity_type` and every live query use
-- the long form, and notes join to those.
ALTER TABLE wiki_notes DROP CONSTRAINT wiki_notes_subject_type_check;
ALTER TABLE wiki_notes ADD  CONSTRAINT wiki_notes_subject_type_check
    CHECK (subject_type IN ('event', 'day', 'story', 'person', 'place',
                            'organization', 'chat', 'page', 'narrative_identity'));

-- Any rows already using the short form (there are none on a real box, but a
-- migration must not assume its own dataset).
UPDATE wiki_notes SET subject_type = 'organization' WHERE subject_type = 'org';

-- ---------------------------------------------------------------------------
-- What a machine note has to carry
-- ---------------------------------------------------------------------------
-- Citations, so a note can be checked rather than believed. A note saying
-- "Sarah may have moved to Denver — group thread, Jul 12, tone ambiguous" is
-- useful EVEN WHEN WRONG, because the link makes it checkable in seconds. A
-- bare claim is worthless when wrong. That asymmetry is the whole design, so
-- the requirement is a constraint rather than a prompt instruction.
ALTER TABLE wiki_notes ADD COLUMN source_refs JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE wiki_notes ADD CONSTRAINT wiki_notes_machine_must_cite
    CHECK (author = 'human' OR jsonb_array_length(source_refs) > 0);

-- The three exits. Notes never age out: a note whose purpose is "prose for
-- later" that deletes itself before later arrives has defeated itself, and
-- silently, which is worse. So resolution is an event, never a timer.
--
--   accepted  — a human folded it into the article
--   dismissed — a human said no
--   absorbed  — an article rewrite incorporated it
--
-- `absorbed` must be REPORTED, not inferred: a maintenance edit knows which
-- notes it was given, never whether the text it emitted reflects any one of
-- them. The editor stamps the ids it reports using. Without that, absorption
-- never fires and "the backlog is growing because the writer's bar is too low"
-- becomes unfalsifiable — growth would be equally explained by a missing exit.
ALTER TABLE wiki_notes ADD COLUMN resolved_at TIMESTAMPTZ;
ALTER TABLE wiki_notes ADD COLUMN resolution  TEXT
    CHECK (resolution IN ('accepted', 'dismissed', 'absorbed'));
ALTER TABLE wiki_notes ADD COLUMN resolved_by TEXT
    CHECK (resolved_by IN ('ai', 'human'));

-- The two cannot disagree.
ALTER TABLE wiki_notes ADD CONSTRAINT wiki_notes_resolution_pair
    CHECK ((resolved_at IS NULL) = (resolution IS NULL));

-- The open-notes read, and the badge count. Replaces idx_wiki_notes_subject,
-- which this is a prefix of.
DROP INDEX idx_wiki_notes_subject;
CREATE INDEX idx_wiki_notes_open ON wiki_notes (subject_type, subject_id)
    WHERE resolved_at IS NULL;

-- ---------------------------------------------------------------------------
-- app_annotations → app_marginalia
-- ---------------------------------------------------------------------------
ALTER TABLE app_annotations RENAME TO app_marginalia;
ALTER INDEX idx_app_annotations_file RENAME TO idx_app_marginalia_file;
ALTER INDEX app_annotations_pkey     RENAME TO app_marginalia_pkey;

-- The index rows must move with the table, in this migration.
--
-- There is NO GC path in the search layer: nothing ever deletes rows whose
-- ontology no longer exists. And the subtler half — `source_table` is only
-- rewritten inside the indexer's upsert, which fires only when `doc_hash`
-- changes. A rename does not change md5(embed_text), so without this the old
-- value would stick FOREVER, and `search/query.rs` joins entity filters on it.
UPDATE search_embeddings
   SET ontology = 'document_marginalia', source_table = 'app_marginalia'
 WHERE ontology = 'document_annotation';

-- ---------------------------------------------------------------------------
-- wiki_people.notes is NOT dropped here
-- ---------------------------------------------------------------------------
-- It is superseded by wiki_notes, and it is empty on a real box — but three
-- live queries still select it, two of them `sqlx::query!` macros. Dropping it
-- in the same migration that supersedes it is precisely the failure the drop
-- rule exists to prevent: the offline query cache would still claim the column
-- exists, `cargo check` and CI would stay green, and the entity endpoints would
-- 500 on a box.
--
-- So this migration adds the replacement and leaves the column alone. The code
-- stops reading it in this phase; a later migration drops it, after the release
-- that `virtues rollback` would land on has already stopped reading it too.
