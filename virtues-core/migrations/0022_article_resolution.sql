-- Article resolution: one process writes and maintains every subject's
-- article. See agents/plan/article-resolution-plan.md.
--
-- What this replaces. Maintenance was three columns that never worked
-- together: `dirty_at` (stamped by four call sites, read by none),
-- `refresh_after_new_refs` (read by one query, written by nothing) and
-- `source_ref_count` (set on first write only). The sweep that joined them
-- returned Ok(0) by construction and its applet shipped disabled. None of the
-- three is dropped here — a dropped column stops the previous binary booting,
-- which takes `virtues upgrade`'s rollback with it, so every drop waits for
-- one terminal migration after boxes have rolled.

-- ── Who maintains an article ────────────────────────────────────────────────
--
-- `auto_update` was a boolean with a hidden second meaning: the Yjs layer
-- flipped it to false on the first edit that changed the text, which froze the
-- article ("the record stops updating it"). That is the one-pen rule, and
-- article resolution overrules it — almost nobody wants to maintain their own
-- record; they want to leave marginalia and touch a sentence without taking
-- the pen. So the flip goes, and what protects their words is that the editor
-- is shown them and refuses an edit that loses them.
--
-- The replacement is a setting the person owns, not a flag the machine trips:
ALTER TABLE wiki_articles ADD COLUMN maintenance text NOT NULL DEFAULT 'auto'
    CONSTRAINT wiki_articles_maintenance_check
    CHECK (maintenance IN ('always', 'auto', 'never'));

-- Carrying the old state across, conservatively. An article already frozen
-- becomes 'never' rather than 'auto'. It was frozen by an automatic flip
-- rather than a decision, so 'auto' is arguably the truer reading of the new
-- doctrine — but a person who edited a page and was told the record would stop
-- updating it must not find it edited again because they upgraded. They can
-- set it back on the page. Going forward nothing becomes 'never' on its own.
UPDATE wiki_articles SET maintenance = 'never' WHERE auto_update = false;

-- ── When an article is due ──────────────────────────────────────────────────
--
-- Drift, not a counter (the doctrine in agents/plan/attention-plan.md): hash
-- what the article rests on, and revisit when that hash moves. `wiki_days`
-- already gates re-segmentation this way with `sources_fingerprint`; this is
-- the same predicate wired to trigger work rather than only to skip it.
ALTER TABLE wiki_articles ADD COLUMN input_fingerprint text;

-- "Update now". Also settable in bulk, which is the only way a changed
-- constitution or brief can reach articles that are already written: their
-- inputs have not moved, so drift alone would never revisit them.
ALTER TABLE wiki_articles ADD COLUMN update_requested_at timestamp with time zone;

-- Stamped by the websocket hook that used to trip the claim flip. The editor
-- skips an article somebody was in minutes ago: safe under the CRDT, still
-- jarring to watch a paragraph change under the cursor.
ALTER TABLE wiki_articles ADD COLUMN last_human_edit_at timestamp with time zone;

-- ── Whose words are whose ───────────────────────────────────────────────────
--
-- `machine_text` is the article exactly as the editor last left it. The
-- difference between it and the live text is precisely what the person has
-- done since — so their sentences and their deletions are derivable without
-- per-character attribution, and without trusting the version history (whose
-- authors are 'user'/'ai'/'auto', which no wiki component writes, and which is
-- pruned at 50 rows per page — it would forget the oldest human sentence
-- first, which is the one most worth keeping).
--
-- `theirs` and `removed` are arrays of sentences: what must survive an edit
-- verbatim, and what must never come back. The editor is shown both, and the
-- server refuses an edit that violates either.
ALTER TABLE wiki_articles ADD COLUMN machine_text text;
ALTER TABLE wiki_articles ADD COLUMN theirs jsonb NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE wiki_articles ADD COLUMN removed jsonb NOT NULL DEFAULT '[]'::jsonb;

-- The editor's due query: everything the person has not switched off, oldest
-- edition first. Small table, but this is the one query that runs hourly.
CREATE INDEX idx_wiki_articles_due ON wiki_articles (last_written_at)
    WHERE maintenance <> 'never';

-- ── One vocabulary of subjects ──────────────────────────────────────────────
--
-- Three tables spell "what a subject is" and all three disagreed. Articles and
-- notes gain 'year'; rules gain everything, because a rule scoped to a page is
-- how a person says "don't write that again" about one article, and today it
-- can only be said about a person, a place or an organization.
--
-- `wiki_refs.entity_type` is deliberately NOT widened. It answers a different
-- question — what a record points AT — and a year is not something a message
-- references. Keeping it narrow is what stops refs becoming a junk drawer.
ALTER TABLE wiki_articles DROP CONSTRAINT wiki_articles_subject_type_check;
ALTER TABLE wiki_articles ADD CONSTRAINT wiki_articles_subject_type_check
    CHECK (subject_type = ANY (ARRAY[
        'person'::text, 'place'::text, 'organization'::text, 'day'::text,
        'story'::text, 'narrative_identity'::text, 'chapter'::text, 'year'::text
    ]));

ALTER TABLE wiki_notes DROP CONSTRAINT wiki_notes_subject_type_check;
ALTER TABLE wiki_notes ADD CONSTRAINT wiki_notes_subject_type_check
    CHECK (subject_type = ANY (ARRAY[
        'event'::text, 'day'::text, 'story'::text, 'person'::text,
        'place'::text, 'organization'::text, 'chat'::text, 'page'::text,
        'narrative_identity'::text, 'chapter'::text, 'year'::text
    ]));

ALTER TABLE wiki_rules DROP CONSTRAINT wiki_rules_subject_type_check;
ALTER TABLE wiki_rules ADD CONSTRAINT wiki_rules_subject_type_check
    CHECK (subject_type IS NULL OR subject_type = ANY (ARRAY[
        'person'::text, 'place'::text, 'organization'::text, 'day'::text,
        'story'::text, 'narrative_identity'::text, 'chapter'::text, 'year'::text
    ]));

-- ── The year gains the one field only the person can write ──────────────────
--
-- `wiki_years` has had zero code references since it was created, so it is
-- reshaped rather than migrated. `title` already exists and is theirs.
-- `summary` is the same idiom as `wiki_chapters.summary`: their sentence about
-- the year, verbatim, never paraphrased by the editor. The columns that go
-- (`description`, `content`, `cover_image`) wait for the terminal drop, like
-- every other one.
ALTER TABLE wiki_years ADD COLUMN summary text;
