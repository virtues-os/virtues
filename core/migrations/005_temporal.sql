-- Temporal: Timeline chunks, embeddings, and vector search
-- Consolidates: 006, 007, 009

--------------------------------------------------------------------------------
-- ENUMS FOR TIMELINE
--------------------------------------------------------------------------------

-- Enum for chunk types (idempotent)
DO $$ BEGIN
    CREATE TYPE data.chunk_type AS ENUM ('location', 'transit', 'missing_data');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

-- Enum for missing data reasons (idempotent)
DO $$ BEGIN
    CREATE TYPE data.missing_reason AS ENUM ('sleep', 'indoors', 'phone_off', 'unknown');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

--------------------------------------------------------------------------------
-- TIMELINE CHUNK (pre-computed location-first day chunks)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.timeline_chunk (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Core Fields (All chunk types)
    chunk_type data.chunk_type NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    duration_minutes INTEGER GENERATED ALWAYS AS
        (EXTRACT(EPOCH FROM (end_time - start_time)) / 60) STORED,

    -- Location Chunk Fields
    place_id UUID REFERENCES data.entities_place(id),
    place_name TEXT,                    -- Snapshot of place name
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION,
    is_known_place BOOLEAN,

    -- Transit Chunk Fields
    distance_km DOUBLE PRECISION,
    avg_speed_kmh DOUBLE PRECISION,
    from_place TEXT,                    -- Snapshot of origin place name
    to_place TEXT,                      -- Snapshot of destination place name

    -- Missing Data Chunk Fields
    likely_reason data.missing_reason,
    last_known_location TEXT,
    next_known_location TEXT,

    -- Attached Ontology Data (Generic JSONB)
    -- Format: {"messages": [...], "transcripts": [...], "calendar_events": [...]}
    attached_data JSONB NOT NULL DEFAULT '{}',

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Constraints (idempotent)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'valid_duration') THEN
        ALTER TABLE data.timeline_chunk ADD CONSTRAINT valid_duration CHECK (end_time > start_time);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'location_requires_coords') THEN
        ALTER TABLE data.timeline_chunk ADD CONSTRAINT location_requires_coords CHECK (
            chunk_type != 'location' OR (latitude IS NOT NULL AND longitude IS NOT NULL)
        );
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'transit_requires_distance') THEN
        ALTER TABLE data.timeline_chunk ADD CONSTRAINT transit_requires_distance CHECK (
            chunk_type != 'transit' OR distance_km IS NOT NULL
        );
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'unique_chunk_time') THEN
        ALTER TABLE data.timeline_chunk ADD CONSTRAINT unique_chunk_time UNIQUE (chunk_type, start_time, end_time);
    END IF;
END $$;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_timeline_chunk_time
    ON data.timeline_chunk(start_time DESC);

CREATE INDEX IF NOT EXISTS idx_timeline_chunk_time_range
    ON data.timeline_chunk USING GIST (
        tstzrange(start_time, end_time, '[]')
    );

CREATE INDEX IF NOT EXISTS idx_timeline_chunk_type
    ON data.timeline_chunk(chunk_type);

CREATE INDEX IF NOT EXISTS idx_timeline_chunk_place
    ON data.timeline_chunk(place_id)
    WHERE place_id IS NOT NULL;

-- Trigger
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

--------------------------------------------------------------------------------
-- EMBEDDINGS: ONTOLOGY TABLES (from 007)
--------------------------------------------------------------------------------

-- Social Email embeddings
ALTER TABLE data.social_email
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_social_email_embedding
ON data.social_email USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Social Message embeddings
ALTER TABLE data.social_message
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_social_message_embedding
ON data.social_message USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Praxis Calendar embeddings
ALTER TABLE data.praxis_calendar
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_praxis_calendar_embedding
ON data.praxis_calendar USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Knowledge AI Conversation embeddings
ALTER TABLE data.knowledge_ai_conversation
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conv_embedding
ON data.knowledge_ai_conversation USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

--------------------------------------------------------------------------------
-- EMBEDDINGS: KNOWLEDGE DOCUMENT (from 009)
--------------------------------------------------------------------------------

ALTER TABLE data.knowledge_document
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_knowledge_document_embedding
ON data.knowledge_document USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

--------------------------------------------------------------------------------
-- EMBEDDING SETTINGS IN ASSISTANT PROFILE (from 007)
--------------------------------------------------------------------------------

ALTER TABLE app.assistant_profile
ADD COLUMN IF NOT EXISTS embedding_model_id TEXT DEFAULT 'nomic-embed-text',
ADD COLUMN IF NOT EXISTS ollama_endpoint TEXT DEFAULT 'http://localhost:11434';

--------------------------------------------------------------------------------
-- EMBEDDING JOBS (from 007)
--------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS data.embedding_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_table TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    records_processed INTEGER DEFAULT 0,
    records_total INTEGER,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Constraint (idempotent)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'embedding_jobs_status_check') THEN
        ALTER TABLE data.embedding_jobs ADD CONSTRAINT embedding_jobs_status_check
        CHECK (status IN ('pending', 'running', 'completed', 'failed'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_embedding_jobs_status
ON data.embedding_jobs(status, created_at DESC);
