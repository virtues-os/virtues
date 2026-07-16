-- 0046: kind + confidence — the two structured axes of a timeline block.
--
-- `kind` is the SINGLE source of truth for a block's classification, replacing the
-- three mutually-exclusive booleans (is_unknown / is_transit / is_sleep) that let
-- illegal combinations (`unknown AND transit`) be represented at all. See
-- docs/event-timeline.md ("The structured axes: kind, confidence, salience").
--
-- The booleans are KEPT as GENERATED columns derived from `kind`, so every existing
-- reader — Rust queries, the `TemporalEvent` API serialization, the frontend, even
-- ad-hoc SQL — keeps working untouched. Only WRITERS move to `kind` (they set it;
-- they cannot write a generated column). A later migration drops the generated
-- booleans once readers + the frontend migrate to `kind`.
--
-- `confidence` (low / medium / high) is how sure we are of the block: deterministic
-- for events (witness agreement — how many independent sources corroborate the
-- window), anchored coverage for the day. An enum, not a 1-5 scale, to avoid false
-- precision and cross-model drift.

-- 1. The source-of-truth column, backfilled from the outgoing booleans.
ALTER TABLE wiki_events
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'stay'
        CHECK (kind IN ('stay', 'transit', 'sleep', 'unknown'));

UPDATE wiki_events SET kind = CASE
    WHEN is_sleep   THEN 'sleep'
    WHEN is_unknown THEN 'unknown'
    WHEN is_transit THEN 'transit'
    ELSE 'stay'
END;

-- 2. Replace the booleans with views generated from `kind` (readers unaffected).
ALTER TABLE wiki_events
    DROP COLUMN is_unknown,
    DROP COLUMN is_transit,
    DROP COLUMN is_sleep;

ALTER TABLE wiki_events
    ADD COLUMN is_unknown BOOLEAN NOT NULL GENERATED ALWAYS AS (kind = 'unknown') STORED,
    ADD COLUMN is_transit BOOLEAN NOT NULL GENERATED ALWAYS AS (kind = 'transit') STORED,
    ADD COLUMN is_sleep   BOOLEAN NOT NULL GENERATED ALWAYS AS (kind = 'sleep')   STORED;

-- 3. Confidence: nullable until the annotate step computes it from witness count.
ALTER TABLE wiki_events
    ADD COLUMN confidence TEXT CHECK (confidence IN ('low', 'medium', 'high'));
