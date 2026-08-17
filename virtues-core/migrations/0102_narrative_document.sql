-- The long form of "In your own words", beside the short one.
--
-- `wiki_narrative_identity.content` is the DISTILLED core: 60-110 words, and it
-- is injected into every chat prompt. That is why it is short, and why the
-- document a person actually reads cannot live there — a few thousand words in
-- that column would ride along on every message they ever send.
--
-- So two artifacts from one set of answers:
--
--   document — past / present / future, in their voice, for them to read and
--              correct. Long, and never injected wholesale.
--   content  — the distilled core, for the prompt. Derived from `document`.
--
-- THE LONG-TERM HOME IS A PAGE. `wiki_articles` already joins a subject to an
-- `app_pages` row, which brings Yjs editing, `app_page_versions` for history,
-- and `auto_update` as the AI-consent switch — the first-copy/second-copy
-- lineage this needs, already built and already used by every other article.
-- This column is the smaller step: it gets the document written and stored
-- while that wiring is done, and moving it later is a backfill rather than a
-- redesign.

ALTER TABLE wiki_narrative_identity
    ADD COLUMN IF NOT EXISTS document TEXT NOT NULL DEFAULT '';

-- When the document was last drafted from the interview answers. Distinct from
-- `updated_at`, which moves when a person edits their own prose: regenerating
-- should be offered when there are NEW answers since the last draft, not
-- whenever the row was touched.
ALTER TABLE wiki_narrative_identity
    ADD COLUMN IF NOT EXISTS drafted_at TIMESTAMPTZ;
