-- Migration: Add pgvector embeddings for semantic search
-- Adds embedding columns to ontology tables and HNSW indexes

-- ============================================================================
-- Add embedding columns to ontology tables (768 dims for nomic-embed-text)
-- ============================================================================

ALTER TABLE data.social_email
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

ALTER TABLE data.social_message
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

ALTER TABLE data.praxis_calendar
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

ALTER TABLE data.knowledge_ai_conversation
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

-- ============================================================================
-- HNSW indexes for fast similarity search (partial - only non-null embeddings)
-- Note: Using IF NOT EXISTS and removing CONCURRENTLY for migration compatibility
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_social_email_embedding
ON data.social_email USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

CREATE INDEX IF NOT EXISTS idx_social_message_embedding
ON data.social_message USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

CREATE INDEX IF NOT EXISTS idx_praxis_calendar_embedding
ON data.praxis_calendar USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

CREATE INDEX IF NOT EXISTS idx_knowledge_ai_conv_embedding
ON data.knowledge_ai_conversation USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- ============================================================================
-- Add embedding settings to assistant_profile
-- ============================================================================

ALTER TABLE app.assistant_profile
ADD COLUMN IF NOT EXISTS embedding_model_id TEXT DEFAULT 'nomic-embed-text',
ADD COLUMN IF NOT EXISTS ollama_endpoint TEXT DEFAULT 'http://localhost:11434';

-- ============================================================================
-- Job tracking for embedding batches
-- ============================================================================

CREATE TABLE IF NOT EXISTS data.embedding_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_table TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    records_processed INTEGER DEFAULT 0,
    records_total INTEGER,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_embedding_jobs_status
ON data.embedding_jobs(status, created_at DESC);
