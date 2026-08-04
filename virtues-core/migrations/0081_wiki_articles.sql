-- 0081 — Articles: one prose store for every subject in the wiki.
--
-- The wiki had six independent implementations of "prose about a subject":
-- `article`/`content`/`notes` columns on people, places and orgs (0072),
-- `autobiography` on days, `content` on stories, and one on
-- wiki_narrative_identity. None had revision history, an on/off switch for AI
-- rewriting, or a shared editor.
--
-- `app_pages` has all three already — Yjs editing, `app_page_versions` with a
-- `created_by` column, and an AI write path that goes through the CRDT rather
-- than around it. So an article IS a page, and the wiki owns a join row rather
-- than a second copy of the prose. A `wiki_articles.content` column would mean
-- a second AI-write path with no CRDT reconciliation and no pre-edit snapshot:
-- strictly worse than the one that already works.
--
-- Nothing is created here. Articles are opt-in (a person clicks "Write the
-- article"), and there is nothing to backfill: `entity_article_gen` has never
-- produced an article on a real box. The 13 `wiki_days.autobiography` rows are
-- the only prose that has to move, and they move in their own migration.

-- ---------------------------------------------------------------------------
-- Which pages are articles
-- ---------------------------------------------------------------------------
-- A discriminator on the page rather than an anti-join from the wiki. Both
-- encode the same fact, but a predicate someone forgets to write is a leak,
-- and a column with a default is not.
ALTER TABLE app_pages ADD COLUMN kind TEXT NOT NULL DEFAULT 'page';

COMMENT ON COLUMN app_pages.kind IS
    '''page'' = a document a person made; ''article'' = the record''s prose about a subject (see wiki_articles). Articles are hidden from the Pages list and tree, and indexed under a separate ontology.';

-- NOT a btree on `kind`. It has two values, and the selectivity INVERTS over
-- time: 18 pages and 0 articles today, but hundreds of articles later, so
-- `kind = 'page'` becomes the rare value. The planner would use it for neither.
-- What the Pages list actually runs is ordered, so scope the existing access
-- paths instead — the repo's dominant idiom (0006, 0033).
CREATE INDEX idx_pages_updated_pages ON app_pages (updated_at DESC) WHERE kind = 'page';
CREATE INDEX idx_pages_title_pages   ON app_pages (title)           WHERE kind = 'page';

-- ---------------------------------------------------------------------------
-- The join row
-- ---------------------------------------------------------------------------
CREATE TABLE wiki_articles (
    id            TEXT PRIMARY KEY,

    -- 'organization', NOT 'org'. Three vocabularies exist in the tree for this
    -- one concept: wiki_entity_refs.entity_type says 'organization' and every
    -- live query agrees, while wiki_marginalia.subject_type says 'org' and the
    -- frontend route is /org. New schema follows the query layer, because the
    -- article sweep and the entity-index ranking both JOIN wiki_articles to
    -- wiki_entity_refs — and 'org' would make those joins silently return zero
    -- org rows. The route stays /org; the mapping happens once, at the edge.
    subject_type  TEXT NOT NULL CHECK (subject_type IN
                    ('person','place','organization','day','story','narrative_identity')),

    -- No FK, and none is possible: this points at six different tables. The
    -- delete guards therefore live in the entity-delete handlers, one per
    -- subject type. Deleting the PAGE cascades this row away; deleting the
    -- SUBJECT does not, which is why those handlers have to say so out loud.
    subject_id    TEXT NOT NULL,

    page_id       TEXT NOT NULL UNIQUE REFERENCES app_pages(id) ON DELETE CASCADE,

    -- OFF by default, and it means the AI never touches this article — not a
    -- pending-approval queue, not a review inbox. The maintenance sweep simply
    -- skips it. The switch IS the consent; History plus revert is the review
    -- surface for articles that have it on.
    auto_update   BOOLEAN NOT NULL DEFAULT false,

    -- How much new evidence justifies a rewrite, per article. Replaces the
    -- hardcoded MIN_NEW_REFS = 10 that applied to every entity on the box.
    -- CHECKed above zero: at zero or below, every sweep would rewrite every
    -- auto-update article forever — an unbounded model spend, which is the
    -- exact failure the opt-in design exists to prevent.
    refresh_after_new_refs INTEGER NOT NULL DEFAULT 10
                    CHECK (refresh_after_new_refs > 0),

    -- The queue. Authoritative for prose staleness; the `dirty_at` columns 0033
    -- put on wiki_events/days/stories mean "new evidence about a settled
    -- object" and are written today only by the magnet (to mean a stale
    -- centroid). Do not conflate them.
    dirty_at         TIMESTAMPTZ,

    -- What the evidence looked like at the last write, so the sweep can gate on
    -- growth instead of a timer. Moved off the entity tables, where 0072 put it.
    source_ref_count INTEGER NOT NULL DEFAULT 0,
    last_written_at  TIMESTAMPTZ,

    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_wiki_articles_subject ON wiki_articles (subject_type, subject_id);
CREATE INDEX idx_wiki_articles_dirty ON wiki_articles (dirty_at) WHERE dirty_at IS NOT NULL;

-- Singletons need their own constraint. UNIQUE(subject_type, subject_id) does
-- NOT stop two narrative-identity articles carrying different subject_ids —
-- and wiki_narrative_identity has no uniqueness of its own either (unlike
-- wiki_telos, which has idx_narrative_telos_single_active). Same idiom as that
-- one.
CREATE UNIQUE INDEX idx_wiki_articles_singleton ON wiki_articles (subject_type)
    WHERE subject_type = 'narrative_identity';

-- Five of these columns are mutable and every wiki_* table in 0006 carries the
-- trigger.
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_articles
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();
