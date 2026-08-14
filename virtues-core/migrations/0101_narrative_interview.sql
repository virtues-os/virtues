-- The interview behind "In your own words".
--
-- `wiki_narrative_identity` holds the FINISHED prose — one row, the document a
-- person reads and the assistant is given. This holds the raw material it was
-- drafted from: one row per question, in their own words.
--
-- Kept separate rather than folded into the document for three reasons. The
-- draft can be regenerated when the writing improves, without asking anyone to
-- answer twelve questions again. An answer can be revised on its own, years
-- later, without editing a paragraph that wove it together with three others.
-- And the answers are the person's actual words, where the document is a
-- machine's arrangement of them — those deserve different durability.
--
-- AUTOSAVE IS THE POINT. This is an hour of writing about grief, vice and
-- family. Losing it to a reload is not an inconvenience, it is a betrayal on
-- the one document where trust matters most, so rows are written as the person
-- types rather than on submit.

CREATE TABLE wiki_narrative_interview (
    -- The question's stable id (`places`, `chapters`, `loss`, …), not its
    -- position: the set will be reordered and reworded, and an answer about
    -- losing a parent must never end up filed under a question about hobbies.
    question_id TEXT PRIMARY KEY,
    answer      TEXT NOT NULL DEFAULT '',
    -- Cheap progress + the interviewer's "want to say more?" heuristic without
    -- re-counting prose on every read.
    word_count  INTEGER NOT NULL DEFAULT 0,
    -- Set when the person moves on having written something real. Distinct from
    -- a non-empty answer, which may be three words typed while thinking.
    completed_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_narrative_interview
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- Standing orders — the enforced half.
--
-- "I'm a recovering alcoholic; never suggest bars" cannot live in prose and
-- depend on a model noticing it every time. The cost of missing it once is
-- unacceptable, so it is a rule with the reliability of a row, extracted only
-- when the person confirms it should be one. Never inferred.
CREATE TABLE wiki_standing_order (
    id         TEXT PRIMARY KEY,
    -- The rule, in the person's own words where possible.
    rule       TEXT NOT NULL,
    -- `avoid` = never raise this unprompted. `defend` = actively help me hold
    -- this line. The two need opposite handling and a single "be careful"
    -- switch can express neither.
    kind       TEXT NOT NULL DEFAULT 'avoid' CHECK (kind IN ('avoid', 'defend')),
    active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_standing_order
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();
