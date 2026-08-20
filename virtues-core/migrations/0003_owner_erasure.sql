-- The owner asking the box to forget something.
--
-- No table expressed this, and three existing concepts each look like it and are
-- not:
--
--   `deleted_at_source`  means the UPSTREAM no longer has it (Gmail deleted the
--                        mail). Says nothing about what the owner wants.
--   `is_archived`        in-app triage. The row stays and stays searchable.
--   `wiki_rules`         ('avoid'/'defend') governs what the ASSISTANT WRITES
--                        ABOUT. Editorial, not physical — an "avoid" rule must
--                        never silently delete the underlying record, and a
--                        deletion must never be satisfied by merely not
--                        mentioning it. Keeping the two apart is the point.
--
-- So today there is no answer to "delete every recording from Tuesday
-- afternoon", "never ingest anything from this contact", or "purge everything
-- that came from this credential". The only deletion in the codebase is the
-- sweeper's blind 90-day retention on auth events.
--
-- ## Why this lands now, empty
--
-- The record is fanned across 26 `data_*` tables, the lake on disk, three search
-- indexes (`search_embeddings`, `search_vectors`, `search_bm25_postings`),
-- extracted document chunks, `wiki_refs`, and encrypted backup increments the
-- box cannot itself read back. A deletion that is not modeled cannot be executed
-- correctly across eight layers, and — worse — cannot be AUDITED. "Did it
-- actually leave the search index?" needs a per-layer record, or the answer is a
-- shrug.
--
-- Reserving the table costs nothing and closes permanently once units are in
-- homes: retrofitting erasure into a system with eight derived copies of
-- everything, with real people's data live in it, is the migration you least
-- want to write. The sweeper that acts on these rows can ship whenever.
--
-- `app_` and not `data_`: this is the owner's INSTRUCTION, which is product
-- state, the same way `app_pins` is product state even though it points at wiki
-- entities. The prefix describes what the row IS, not what it points at.

CREATE TABLE app_erasure (
    id            text PRIMARY KEY,

    -- WHAT is being erased. Each scope uses a different target column below,
    -- enforced by the CHECK at the end: a request that names nothing is not a
    -- request, and finding that out when the sweeper runs is too late.
    scope         text NOT NULL CHECK (scope IN
                    ('record', 'time_range', 'entity', 'credential', 'stream')),

    -- scope = 'record': one row, addressed the way every citation addresses one
    -- (`wiki_refs` uses the same pair).
    source_table  text,
    source_id     text,

    -- scope = 'time_range': everything the box recorded in a window. Half-open
    -- [starts_at, ends_at) so adjacent ranges cannot double-cover an instant.
    starts_at     timestamptz,
    ends_at       timestamptz,

    -- scope = 'entity': a person, place or organization, and everything that
    -- refers to them. "This person asked not to be recorded."
    entity_id     text,

    -- scope = 'credential': everything that arrived through one connection.
    -- "I am disconnecting this account and I want what it brought with it gone."
    credential_id text REFERENCES credentials(id) ON DELETE SET NULL,

    -- scope = 'stream': one ingest stream by name (e.g. a microphone stream).
    stream        text,

    -- PURGE deletes what already exists. EXCLUDE is a standing refusal to
    -- ingest, which is the only correct way to honour "never record this
    -- person" — a pre-filter at the door, not a delete afterwards. A request
    -- may be both, in which case it is two rows, deliberately: they complete at
    -- different times and one can succeed while the other is still owed.
    mode          text NOT NULL CHECK (mode IN ('purge', 'exclude')),

    -- The owner's own words, if they gave any. Never required — a person should
    -- not have to justify deleting their own record.
    reason        text,

    -- `created_at` IS the request time. Not `requested_at`: per the naming rule
    -- in CLAUDE.md, created_at means "when we wrote this row", and for a request
    -- the writing and the asking are the same instant. One name per idea.
    created_at    timestamptz NOT NULL DEFAULT now(),

    -- NULL = the sweeper still owes work on this request.
    applied_at    timestamptz,

    -- Which of the eight layers are done, e.g.
    --   {"rows": "2026-08-18T…", "lake": "…", "search": null, "backups": null}
    -- This is what makes an erasure auditable instead of asserted. Without it,
    -- "is it gone from the search index?" has no answer, and a partially applied
    -- erasure is indistinguishable from a complete one.
    applied_scopes jsonb NOT NULL DEFAULT '{}',

    -- A request must actually name its target.
    CONSTRAINT app_erasure_target_matches_scope CHECK (
        CASE scope
            WHEN 'record'     THEN source_table IS NOT NULL AND source_id IS NOT NULL
            WHEN 'time_range' THEN starts_at IS NOT NULL AND ends_at IS NOT NULL
                                   AND ends_at > starts_at
            WHEN 'entity'     THEN entity_id IS NOT NULL
            WHEN 'credential' THEN credential_id IS NOT NULL
            WHEN 'stream'     THEN stream IS NOT NULL
        END
    )
);

COMMENT ON TABLE app_erasure IS
    'Owner-directed erasure: purge what exists, or stand as a refusal to ingest. Distinct from deleted_at_source (upstream dropped it), is_archived (triage) and wiki_rules (what the assistant writes about).';

-- The outstanding queue: what the sweeper asks for on every tick.
CREATE INDEX idx_app_erasure_pending ON app_erasure (created_at)
    WHERE applied_at IS NULL;

-- The ingest pre-filter. Once `mode='exclude'` is honoured this is the hot
-- path — consulted before writing an ingested row — so it is indexed from the
-- start rather than discovered later under load.
CREATE INDEX idx_app_erasure_exclude ON app_erasure (scope, entity_id, stream)
    WHERE mode = 'exclude';
