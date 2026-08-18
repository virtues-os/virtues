-- Three near-identical tables stop spelling the same things three ways.
--
-- `wiki_people`, `wiki_places` and `wiki_orgs` share a spine — a name, a count
-- of how often the entity appears, and a first/last sighting — and named every
-- part of it differently:
--
--   people   canonical_name  interaction_count  first_interaction  last_interaction
--   places   name            visit_count        first_visit        last_visit
--   orgs     canonical_name  interaction_count  first_interaction  last_interaction
--
-- The cost was not aesthetic. Every query that fans out across all three had to
-- write `canonical_name as name`, in `api/pages.rs`, `api/notebooks.rs` and
-- everywhere else that lists entities together — an alias whose only job was to
-- undo a naming decision.
--
--   name, seen_count, first_seen, last_seen
--
-- `seen_count`, NOT `ref_count`. Both originals counted the same thing — how
-- many times the owner encountered this entity — so they should share a name.
-- But `ref_count` is already taken, and by a better claim: the API structs carry
-- a COMPUTED `ref_count` derived from `wiki_refs` ("how many records mention
-- this entity"). These stored columns are a denormalized per-kind counter that
-- can drift from that; giving them the authoritative name would have hidden the
-- difference. `seen_count` also pairs with `first_seen`/`last_seen`, so the
-- three read as one triple.
--
-- `first_seen`/`last_seen` because "interaction" and "visit" are the same fact
-- about different kinds of entity, and the kind is already in the table.
--
-- This is deliberately the CHEAP HALF of merging the three tables into one
-- `wiki_entity`. The merge is what would let `wiki_refs.entity_id` — the
-- product's central citation edge — finally be a foreign key, since today it
-- cannot point at three tables. That is a multi-day change; this is an
-- afternoon, and it removes the aliasing either way.

ALTER TABLE wiki_people RENAME COLUMN canonical_name    TO name;
ALTER TABLE wiki_people RENAME COLUMN interaction_count TO seen_count;
ALTER TABLE wiki_people RENAME COLUMN first_interaction TO first_seen;
ALTER TABLE wiki_people RENAME COLUMN last_interaction  TO last_seen;

ALTER TABLE wiki_orgs   RENAME COLUMN canonical_name    TO name;
ALTER TABLE wiki_orgs   RENAME COLUMN interaction_count TO seen_count;
ALTER TABLE wiki_orgs   RENAME COLUMN first_interaction TO first_seen;
ALTER TABLE wiki_orgs   RENAME COLUMN last_interaction  TO last_seen;

-- places already had `name`; only the count and the sightings move.
ALTER TABLE wiki_places RENAME COLUMN visit_count TO seen_count;
ALTER TABLE wiki_places RENAME COLUMN first_visit TO first_seen;
ALTER TABLE wiki_places RENAME COLUMN last_visit  TO last_seen;

-- And the dead one, while the tables are open. `article_ref_count` has zero
-- references in any Rust, TypeScript or Svelte file, is not maintained by a
-- trigger (the schema has exactly two, neither touches it), and is a plain
-- DEFAULT 0 nothing increments — an orphan of the half-finished migration that
-- moved entity articles into `app_pages`.
ALTER TABLE wiki_people DROP COLUMN article_ref_count;
ALTER TABLE wiki_places DROP COLUMN article_ref_count;
ALTER TABLE wiki_orgs   DROP COLUMN article_ref_count;
