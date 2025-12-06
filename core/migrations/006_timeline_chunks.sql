-- Timeline Chunks Table
-- Pre-computed location-first day chunks, built continuously by hourly cron job.
-- Replaces on-demand chunk building with stored chunks for fast API queries.
--
-- Design: Single table with chunk_type discriminator, using nullable fields
-- for type-specific data. JSONB for attached ontology data (snapshots).

-- Enum for chunk types (idempotent - handles re-runs gracefully)
DO $$ BEGIN
    CREATE TYPE data.chunk_type AS ENUM ('location', 'transit', 'missing_data');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

-- Enum for missing data reasons (idempotent - handles re-runs gracefully)
DO $$ BEGIN
    CREATE TYPE data.missing_reason AS ENUM ('sleep', 'indoors', 'phone_off', 'unknown');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS data.timeline_chunk (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- ==========================================
    -- Core Fields (All chunk types)
    -- ==========================================
    chunk_type data.chunk_type NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    duration_minutes INTEGER GENERATED ALWAYS AS
        (EXTRACT(EPOCH FROM (end_time - start_time)) / 60) STORED,

    -- ==========================================
    -- Location Chunk Fields
    -- ==========================================
    place_id UUID REFERENCES data.entities_place(id),
    place_name TEXT,                    -- Snapshot of place name
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION,
    is_known_place BOOLEAN,

    -- ==========================================
    -- Transit Chunk Fields
    -- ==========================================
    distance_km DOUBLE PRECISION,
    avg_speed_kmh DOUBLE PRECISION,
    from_place TEXT,                    -- Snapshot of origin place name
    to_place TEXT,                      -- Snapshot of destination place name

    -- ==========================================
    -- Missing Data Chunk Fields
    -- ==========================================
    likely_reason data.missing_reason,
    last_known_location TEXT,
    next_known_location TEXT,

    -- ==========================================
    -- Attached Ontology Data (Generic JSONB)
    -- ==========================================
    -- Format: {"messages": [...], "transcripts": [...], "calendar_events": [...]}
    -- Each key is an ontology domain, value is array of preview objects
    -- This scales without schema changes when adding new ontology domains
    attached_data JSONB NOT NULL DEFAULT '{}',

    -- ==========================================
    -- Metadata
    -- ==========================================
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- ==========================================
    -- Constraints
    -- ==========================================
    CONSTRAINT valid_duration CHECK (end_time > start_time),
    CONSTRAINT location_requires_coords CHECK (
        chunk_type != 'location' OR (latitude IS NOT NULL AND longitude IS NOT NULL)
    ),
    CONSTRAINT transit_requires_distance CHECK (
        chunk_type != 'transit' OR distance_km IS NOT NULL
    ),
    -- Prevent duplicate chunks for same time range
    CONSTRAINT unique_chunk_time UNIQUE (chunk_type, start_time, end_time)
);

-- Indexes for Performance
-- Primary query pattern: chunks within a time range
CREATE INDEX IF NOT EXISTS idx_timeline_chunk_time
    ON data.timeline_chunk(start_time DESC);

-- For overlapping range queries (API: give me chunks for this day)
CREATE INDEX IF NOT EXISTS idx_timeline_chunk_time_range
    ON data.timeline_chunk USING GIST (
        tstzrange(start_time, end_time, '[]')
    );

-- For filtering by chunk type
CREATE INDEX IF NOT EXISTS idx_timeline_chunk_type
    ON data.timeline_chunk(chunk_type);

-- For place lookups
CREATE INDEX IF NOT EXISTS idx_timeline_chunk_place
    ON data.timeline_chunk(place_id)
    WHERE place_id IS NOT NULL;

-- Trigger for updated_at
DROP TRIGGER IF EXISTS update_timeline_chunk_updated_at ON data.timeline_chunk;
CREATE TRIGGER update_timeline_chunk_updated_at
    BEFORE UPDATE ON data.timeline_chunk
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

-- Comments
COMMENT ON TABLE data.timeline_chunk IS 'Pre-computed location-first day chunks. Built by hourly cron job from location_visits.';
COMMENT ON COLUMN data.timeline_chunk.chunk_type IS 'Discriminator: location (at a place), transit (moving), missing_data (no GPS)';
COMMENT ON COLUMN data.timeline_chunk.place_name IS 'Snapshot of place name. Prevents time travel bugs when place entities are renamed.';
COMMENT ON COLUMN data.timeline_chunk.attached_data IS 'Generic ontology attachments: {"messages": [...], "transcripts": [...]}. Keys are domain names, values are preview object arrays.';
COMMENT ON COLUMN data.timeline_chunk.likely_reason IS 'For missing_data chunks: why we think data is missing';
