-- Migration: Add embedding support to knowledge_document table
-- Enables semantic search across Notion documents and other knowledge sources

-- Add embedding columns (768 dims for nomic-embed-text)
ALTER TABLE data.knowledge_document
ADD COLUMN IF NOT EXISTS embedding vector(768),
ADD COLUMN IF NOT EXISTS embedded_at TIMESTAMPTZ;

-- HNSW index for fast similarity search
CREATE INDEX IF NOT EXISTS idx_knowledge_document_embedding
ON data.knowledge_document USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);
