-- The second and last of the drops: columns with a reader and no writer.
--
-- 0024 took what article resolution replaced. This takes the rest — every
-- column here has been read by something for months and written by nothing,
-- ever, on any box. That is a worse state than an unused column, because a
-- column with a reader and no writer does not read as MISSING, it reads as
-- WRONG: zero visits to a place you go weekly, "Interactions on record: 0"
-- handed to the model writing someone's article, an empty "Last Interaction"
-- column down the whole people table, a readiness marker that has never once
-- been drawn.
--
-- Their readers came out first, in the commit before this one. A drop is a
-- one-way door — the previous binary cannot boot afterwards, which takes
-- `virtues upgrade`'s rollback with it for this release — so nothing is
-- dropped here on the strength of looking dead.

-- ── The entity counters ─────────────────────────────────────────────────────
--
-- `seen_count`, `first_seen` and `last_seen` were meant to carry importance and
-- recency. Nothing ever wrote them. The real answer was in `wiki_refs` the
-- whole time: the resolver has been recording every message, visit and
-- transaction that touches an entity since it shipped, so a count and its two
-- dates are one query away. The place page now asks that question instead.
ALTER TABLE wiki_people  DROP COLUMN seen_count, DROP COLUMN first_seen, DROP COLUMN last_seen;
ALTER TABLE wiki_places  DROP COLUMN seen_count, DROP COLUMN first_seen, DROP COLUMN last_seen;
ALTER TABLE wiki_orgs    DROP COLUMN seen_count, DROP COLUMN first_seen, DROP COLUMN last_seen;

-- ── The legacy prose columns ────────────────────────────────────────────────
--
-- An entity's article was moved into `app_pages` so it would get the editor,
-- the history and the marginalia every other page has. These columns were kept
-- as a fallback for entities written before that — a fallback that was never
-- needed, because no box had an article when it landed, and never could be,
-- because nothing has written them since.
ALTER TABLE wiki_people DROP COLUMN article, DROP COLUMN article_updated_at;
ALTER TABLE wiki_places DROP COLUMN article, DROP COLUMN article_updated_at;
ALTER TABLE wiki_orgs   DROP COLUMN article, DROP COLUMN article_updated_at;

-- `notes` on a person predates `wiki_notes`, which is where a note about a
-- subject lives, with its citations and its accept/dismiss. The only thing
-- that ever read this column fed it to the article editor as "the owner's own
-- notes", which were always empty.
ALTER TABLE wiki_people DROP COLUMN notes;

-- The value goes into `metadata->>'google_place_id'` and always has; the
-- column of that name was never written.
ALTER TABLE wiki_places DROP COLUMN google_place_id;

-- ── The day's unwritten half ────────────────────────────────────────────────
--
-- `epigraph` and `data_quality` were parsed out of the narration response by a
-- splitter looking for markers the prompt forbids the model to emit, so both
-- were NULL on every row. `readiness_score`/`readiness_details` had no writer
-- at all, and drew a marker on the dayline chart that has therefore never
-- appeared once. `cover_image` and `snapshot` were never written by anything.
--
-- `last_edited_by` is the one with a real history: it was the freeze flag of
-- the one-pen rule, which article resolution overrules. Ownership no longer
-- flips, so there is nothing for it to record — and the article's version
-- history says who wrote each edition, which is the honest answer it was
-- standing in for.
ALTER TABLE wiki_days
    DROP COLUMN epigraph,
    DROP COLUMN data_quality,
    DROP COLUMN readiness_score,
    DROP COLUMN readiness_details,
    DROP COLUMN cover_image,
    DROP COLUMN snapshot,
    DROP COLUMN last_edited_by;

-- Still deliberately kept, and the reasons have not changed:
--   * `wiki_events.agent_action` — the nightly sleep resolver writes it.
--   * `wiki_events.user_hidden` — unwritten on purpose until the correction UI
--     exists; wiring the UI first would ship "hide" as "destroy".
--   * `wiki_rules.subject_type`/`subject_id` — the hook a page-scoped rule
--     needs, which the editor's constitution depends on.
