-- Axiology Schema Cleanup
--
-- 1. Remove axiology_value table ("value" is the genus/meta-concept, not a species)
-- 2. Remove valence column from preference (valence is implicit in table choice)
-- 3. Add embeddings to all 5 axiology tables for semantic search

-- Step 1: Remove axiology_value table
DROP TABLE IF EXISTS data.axiology_value;

-- Step 2: Remove valence column from preference
ALTER TABLE data.axiology_preference DROP COLUMN IF EXISTS valence;

-- Step 3: Add embedding columns to all 5 axiology tables
ALTER TABLE data.axiology_telos
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

ALTER TABLE data.axiology_virtue
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

ALTER TABLE data.axiology_vice
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

ALTER TABLE data.axiology_preference
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

ALTER TABLE data.axiology_temperament
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

-- Step 4: Create HNSW indexes for similarity search
CREATE INDEX IF NOT EXISTS idx_axiology_telos_embedding
ON data.axiology_telos USING hnsw (embedding vector_cosine_ops);

CREATE INDEX IF NOT EXISTS idx_axiology_virtue_embedding
ON data.axiology_virtue USING hnsw (embedding vector_cosine_ops);

CREATE INDEX IF NOT EXISTS idx_axiology_vice_embedding
ON data.axiology_vice USING hnsw (embedding vector_cosine_ops);

CREATE INDEX IF NOT EXISTS idx_axiology_preference_embedding
ON data.axiology_preference USING hnsw (embedding vector_cosine_ops);

CREATE INDEX IF NOT EXISTS idx_axiology_temperament_embedding
ON data.axiology_temperament USING hnsw (embedding vector_cosine_ops);
