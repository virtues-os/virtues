-- Drop what article resolution replaced, and what never had a writer.
--
-- **A drop is a one-way door.** Once a column is gone the previous binary
-- cannot boot against this database, which takes `virtues upgrade`'s rollback
-- with it for the release that carries this migration. So everything here has
-- had its readers removed in code FIRST — the ordering rule the 2026-08-28
-- schema audit exists to enforce — and anything whose sweep is not finished is
-- deliberately left in place, however dead it looks.
--
-- NOT dropped here, and each for a reason:
--   * `wiki_days.epigraph`/`data_quality`/`last_edited_by`/`cover_image`/
--     `snapshot`/`readiness_*`, and the entity tables' `seen_count`/
--     `first_seen`/`last_seen`/`article`/`article_updated_at`/`notes`/
--     `google_place_id` — their sweep runs through the wire structs and the
--     frontend types and is not finished. A half-swept column is exactly the
--     one that takes a box down.
--   * `wiki_events.agent_action` — the nightly sleep resolver writes it.
--   * `wiki_rules.subject_type`/`subject_id` — unwritten today, but they are
--     the hook a page-scoped rule needs ("don't write that again" about ONE
--     article), which is a channel the editor's constitution depends on.
--   * `wiki_events.user_hidden` — unwritten, and deliberately so until the
--     correction UI lands; wiring the UI first would ship "hide" as "destroy".

-- ── The maintenance columns article resolution replaced ─────────────────────
--
-- `auto_update` was a boolean with a second, hidden meaning: the Yjs layer
-- flipped it false on the first real edit, which froze the article for good.
-- That is the one-pen rule, and ownership no longer flips — `maintenance`
-- (0022) is a setting the person owns instead of a flag their typing trips.
ALTER TABLE wiki_articles DROP COLUMN auto_update;

-- `dirty_at` was stamped by four call sites and read by none. Nothing ever
-- consumed it as a queue, so an article marked dirty stayed dirty forever.
-- What makes an article due now is that its evidence MOVED, measured by the
-- editor's fingerprint at flush time rather than stamped on the way past.
ALTER TABLE wiki_articles DROP COLUMN dirty_at;

-- The growth gate: `refs - source_ref_count >= refresh_after_new_refs`.
-- `refresh_after_new_refs` never had a writer at all, and the sweep that read
-- the pair returned Ok(0) by construction while its applet shipped disabled.
ALTER TABLE wiki_articles DROP COLUMN source_ref_count;
ALTER TABLE wiki_articles DROP COLUMN refresh_after_new_refs;

-- ── The orphan table ────────────────────────────────────────────────────────
--
-- `wiki_narrative_identity` lost `document` and `active` in 0006, when the
-- document moved into the wiki as a real article. What was left had zero reads
-- and zero writes in any language — the only mentions anywhere are three
-- comments explaining that it is not used. The document lives where every
-- other subject's prose lives: a `wiki_articles` row joined to a page.
DROP TABLE IF EXISTS wiki_narrative_identity;

-- ── Story columns that never had a writer ───────────────────────────────────
--
-- `wiki_stories` was created with a full set of magazine furniture and never
-- written to once. A story is a subject the person names: a title, their
-- sentence, optional vague dates, and prose that lives on its page like every
-- other article's. `content` in particular had to go — a second prose store
-- beside the page is the six-way duplication the wiki plan exists to end.
ALTER TABLE wiki_stories
    DROP COLUMN content,
    DROP COLUMN subtitle,
    DROP COLUMN themes,
    DROP COLUMN metadata,
    DROP COLUMN cover_image,
    DROP COLUMN sort_order;

-- ── Year columns that never had a writer ────────────────────────────────────
--
-- Same shape, same reason: a year keeps `title` and `summary` (theirs) and its
-- prose is the article's. `wiki_years` had zero code references of any kind
-- until 0022, so nothing has ever read these.
ALTER TABLE wiki_years
    DROP COLUMN description,
    DROP COLUMN content,
    DROP COLUMN cover_image;
