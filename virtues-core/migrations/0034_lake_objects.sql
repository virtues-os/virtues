-- ---------------------------------------------------------------------------
-- The lake: a raw landing zone (docs/lake-plan.md)
--
-- Until now every ingest action was transform-on-write: the raw payload was
-- parsed, projected into the `data_*` ontology tables, and DROPPED. That made
-- `data_*` the evidence floor, which contradicts the stated doctrine ("you can
-- re-integrate new stories from the evidence, but you cannot recover evidence
-- from stories" — docs/stories-plan.md) and cost us real data: the raw focus
-- events behind data_activity_app_usage are gone (aggregated at ingest), and
-- ~13 of 20 collected iMessage fields — every tapback, read receipt and
-- attachment — are uploaded and discarded on arrival. Device pushes (iOS/Mac
-- webhooks) can never replay: there is no upstream to re-fetch from.
--
-- `lake_objects` is the physical inventory of everything on disk. One row per
-- object; the bytes live under STORAGE_PATH at `storage_key`.
--
--   kind='raw_stream'  an archived, REPLAYABLE action payload (.jsonl)
--   kind='media'       a blob (audio today), stored once and referenced
--   kind='drive'       reserved: user Drive files, not yet inventoried here
--
-- The load-bearing invariant: a raw_stream object IS a valid, self-contained
-- action payload — `{"imessages":[…]}` for mac, `{"stream":"location",
-- "records":[…]}` for ios. Replay therefore re-runs the existing action rather
-- than a second copy of the transform, and the archive is self-validating.
--
-- There is deliberately NO projections/watermark table: one object = one
-- stream = one transform, so it would be bookkeeping about nothing, and
-- re-projecting the entire ontology (65 MB, all sources, all time) is minutes.
-- ---------------------------------------------------------------------------

CREATE TABLE lake_objects (
    id               TEXT PRIMARY KEY,
    kind             TEXT NOT NULL CHECK (kind IN ('raw_stream', 'media', 'drive')),

    -- Where the bytes are, relative to STORAGE_PATH. For raw_stream this is the
    -- StreamKeyBuilder layout: streams/{provider}/{source}/{stream}/date=…/records_{ts}.jsonl
    storage_key      TEXT NOT NULL UNIQUE,

    provider         TEXT NOT NULL,              -- 'ios' | 'mac'
    source_id        TEXT,
    stream_name      TEXT NOT NULL,              -- 'imessages' | 'location' | 'microphone' | …

    record_count     INTEGER NOT NULL DEFAULT 0, -- 0 for media (the object IS the record)
    size_bytes       BIGINT  NOT NULL,

    -- Content hash of the stored bytes. UNIQUE is not a nicety: a batch whose
    -- transform is failing gets retried by the device every 5 min against a
    -- 7-day queue — without this, one broken batch archives ~2,000 identical
    -- copies of itself. We watched exactly that loop run for hours.
    sha256           TEXT NOT NULL UNIQUE,

    content_encoding TEXT NOT NULL DEFAULT 'none' CHECK (content_encoding IN ('none', 'zstd')),

    -- The object's time window. Re-projection deletes the rows a transform owns
    -- within this window before re-running it, so an object with a NULL window
    -- can never be re-projected. Nullable only because media has no window.
    min_timestamp    TIMESTAMPTZ,
    max_timestamp    TIMESTAMPTZ,

    -- The RESIDUAL envelope: every top-level key of the original body that no
    -- transform reads (device_id, sent_at, whatever a client adds next). Exactly
    -- the class of field that gets silently dropped today, and it costs nothing.
    metadata         JSONB NOT NULL DEFAULT '{}',

    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Replay's access pattern: "every object for this stream in this window, oldest first."
CREATE INDEX idx_lake_objects_replay ON lake_objects (provider, stream_name, min_timestamp)
    WHERE kind = 'raw_stream';

-- The lake UI (/api/lake/streams) groups by stream; GC and accounting scan by kind.
CREATE INDEX idx_lake_objects_stream  ON lake_objects (stream_name);
CREATE INDEX idx_lake_objects_kind    ON lake_objects (kind, created_at);
