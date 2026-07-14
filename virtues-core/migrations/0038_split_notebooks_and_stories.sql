-- ---------------------------------------------------------------------------
-- Notebooks and Stories are two things. Split them.
--
-- 0033 renamed the notebook table to `wiki_stories` on the theory that one
-- primitive could be both. It cannot, and the code said so all along: the
-- module is called `api/notebooks.rs` and every query in it reads
-- `FROM wiki_stories`. Half of 0033's columns were permanently NULL, and the
-- foreign key never stopped being called `notebook_id`.
--
--   A NOTEBOOK is a CONTAINER. You put things in it. You know what is in it.
--   Spatial, not temporal. Its job is to hold material and be worked in.
--
--   A STORY is a CLAIM. "I got fat." "I became happier." "I act differently
--   on rainy days." You do not know what is in it — that is the whole point.
--   Its job is to gather the evidence scattered across years and render an
--   account of it, with every assertion linked back to the thing that proves
--   it. Temporal by construction.
--
-- So: the existing table goes back to being `app_notebooks` (it always was
-- one), and `wiki_stories` is created fresh with the shape a story actually
-- needs.
--
-- What survives the trip back: `centroid`, `auto_add_materials`, `dirty_at`.
-- Those three are the MAGNET, and the magnet is the one primitive both halves
-- share — a notebook that fills itself and a story that gathers its evidence
-- are the same vector search.
--
-- What does not: the axiology layer (significance / valence / direction /
-- completable / parent_id) and the story-only fields (origin, abstract, state,
-- last_edited_by, pinned). Significance was killed on its merits — it existed
-- to decide what to say UNPROMPTED, and this system only ever speaks when
-- asked, so it had no consumer. The rest ship dormant, and dormant columns
-- with no writer are how `avg_hr` sat broken for months. They can come back
-- the day something writes them.
-- ---------------------------------------------------------------------------

-- 1. The notebook table goes home --------------------------------------------

ALTER TABLE wiki_stories       RENAME TO app_notebooks;
ALTER TABLE wiki_story_members RENAME TO app_notebook_items;

ALTER INDEX idx_wiki_stories_name      RENAME TO idx_app_notebooks_name;
ALTER INDEX idx_wiki_stories_sort      RENAME TO idx_app_notebooks_sort;
ALTER INDEX idx_story_members_story    RENAME TO idx_notebook_items_notebook;
ALTER INDEX idx_story_members_url      RENAME TO idx_notebook_items_url;
ALTER INDEX idx_wiki_stories_dirty     RENAME TO idx_app_notebooks_dirty;

-- Drop the carcass. `parent_id` carries a self-referencing FK, which goes with
-- the column.
ALTER TABLE app_notebooks
    DROP COLUMN IF EXISTS significance,
    DROP COLUMN IF EXISTS valence,
    DROP COLUMN IF EXISTS direction,
    DROP COLUMN IF EXISTS completable,
    DROP COLUMN IF EXISTS parent_id,
    DROP COLUMN IF EXISTS origin,
    DROP COLUMN IF EXISTS abstract,
    DROP COLUMN IF EXISTS state,
    DROP COLUMN IF EXISTS last_edited_by,
    DROP COLUMN IF EXISTS pinned;

-- Kept, and load-bearing: `auto_add_materials` (the toggle), `centroid` (what
-- it matches against), `dirty_at` (membership changed → recompute).
COMMENT ON COLUMN app_notebooks.auto_add_materials IS
    'The magnet: when on, material resembling this notebook attaches itself.';
COMMENT ON COLUMN app_notebooks.centroid IS
    'Mean of the members'' embeddings; the cold-start seed is name + instructions.';

-- 2. Stories, built for the job ----------------------------------------------

CREATE TABLE wiki_stories (
    id            TEXT PRIMARY KEY,

    -- The claim. `title` names it ("My prayer life"); `thesis` states it ("I
    -- pray more when I am anxious, and less when things go well"). The thesis
    -- is what the evidence is gathered FOR, and what the rendered body argues.
    title         TEXT NOT NULL,
    thesis        TEXT,

    -- Who started it. A `named` story is one you wrote — you are good at
    -- naming your own stories and the machine is not. A `discovered` story is
    -- one the machine noticed and is PROPOSING. It stays a proposal — invisible
    -- outside the review surface — until `accepted_at` is set. The machine
    -- proposes; it never pronounces.
    origin        TEXT NOT NULL DEFAULT 'named'
                  CHECK (origin IN ('named', 'discovered')),
    accepted_at   TIMESTAMPTZ,

    -- The time axis. `ended_at IS NULL` means ONGOING: the magnet keeps
    -- recruiting evidence and the body is re-rendered as it grows. Setting
    -- `ended_at` COMPLETES it: the magnet stops, and the story freezes into an
    -- artifact — citable, exportable, the thing worth keeping.
    started_at    TIMESTAMPTZ,
    ended_at      TIMESTAMPTZ,

    -- The rendered account: generated from the members, every claim citing one
    -- of them by route. Derived, never authored — re-deriving it is cheap, so
    -- new evidence means a better story, not a lost one.
    body          TEXT,
    body_rendered_at TIMESTAMPTZ,

    -- The magnet, same as notebooks.
    auto_add_materials BOOLEAN NOT NULL DEFAULT TRUE,
    centroid      BYTEA,

    -- Salience is DECLARED, never computed. "The story of why I made that
    -- decision" is one week and a handful of emails and may be the most
    -- important thing in a life; "the story of my email volume" is enormous and
    -- worthless. Evidence count and importance are uncorrelated, which is
    -- exactly why no amount of witness-counting can infer this. You pin it.
    pinned        BOOLEAN NOT NULL DEFAULT FALSE,

    -- New evidence has landed since the body was last rendered.
    dirty_at      TIMESTAMPTZ,

    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_wiki_stories_open     ON wiki_stories(ended_at) WHERE ended_at IS NULL;
CREATE INDEX idx_wiki_stories_dirty    ON wiki_stories(dirty_at) WHERE dirty_at IS NOT NULL;
CREATE INDEX idx_wiki_stories_proposed ON wiki_stories(origin)  WHERE accepted_at IS NULL;

-- The evidence. A member is anything the app can address by route — an event,
-- a record, a page, an asset — which is what makes a citation and a member the
-- same primitive: the body cites by emitting a member's route.
--
-- `added_by` keeps the magnet honest. Auto-attached evidence is visibly
-- auto-attached and one-click removable; the machine does not silently
-- restructure what a human declared.
CREATE TABLE wiki_story_members (
    id          BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    story_id    TEXT NOT NULL REFERENCES wiki_stories(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,

    added_by    TEXT NOT NULL DEFAULT 'magnet'
                CHECK (added_by IN ('user', 'magnet')),
    -- Cosine similarity to the centroid at attach time. Kept so a threshold can
    -- be re-tuned against what it actually admitted, rather than guessed at.
    similarity  DOUBLE PRECISION,

    -- When the evidence HAPPENED (not when it was attached) — the story is a
    -- shape in time, so this is the axis it is drawn on.
    occurred_at TIMESTAMPTZ,

    added_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (story_id, url)
);

CREATE INDEX idx_story_members_story ON wiki_story_members(story_id, occurred_at);
CREATE INDEX idx_story_members_url   ON wiki_story_members(url);
