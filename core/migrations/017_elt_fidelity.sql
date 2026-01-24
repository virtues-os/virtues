-- Migration 017: ELT Fidelity and Tiering
-- Adds watermarking, tiering, and connection policy columns to ELT tables.

-- Add tier and connection policy to source connections
ALTER TABLE elt_source_connections ADD COLUMN tier TEXT NOT NULL DEFAULT 'free';
ALTER TABLE elt_source_connections ADD COLUMN connection_policy TEXT NOT NULL DEFAULT 'multi_instance';

-- Add watermarking and sync status to stream connections
ALTER TABLE elt_stream_connections ADD COLUMN earliest_record_at TEXT;
ALTER TABLE elt_stream_connections ADD COLUMN latest_record_at TEXT;
ALTER TABLE elt_stream_connections ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'pending' CHECK (sync_status IN ('pending', 'initial', 'incremental', 'backfilling', 'failed'));

-- Create an index for watermarking lookups
CREATE INDEX IF NOT EXISTS idx_elt_stream_connections_watermarks 
    ON elt_stream_connections(source_connection_id, stream_name, earliest_record_at, latest_record_at);
