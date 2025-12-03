-- Onboarding enhancements for axiology + praxis setup
-- Adds missing fields to praxis_aspiration and sync_strategy to source_connections

-- Add target_date to aspirations for specific deadline dates (vs relative timeframes)
ALTER TABLE data.praxis_aspiration
ADD COLUMN IF NOT EXISTS target_date TIMESTAMPTZ;

-- Add source tracking to aspirations (already exists on tasks/initiatives)
ALTER TABLE data.praxis_aspiration
ADD COLUMN IF NOT EXISTS source_provider TEXT DEFAULT 'internal';

ALTER TABLE data.praxis_aspiration
ADD COLUMN IF NOT EXISTS external_id TEXT;

ALTER TABLE data.praxis_aspiration
ADD COLUMN IF NOT EXISTS external_url TEXT;

-- Add sync_strategy to source_connections
-- Determines how imported data relates to ongoing sync:
--   'migration' = one-time import, user will use Virtues going forward
--   'ongoing'   = keep syncing from external source (default, existing behavior)
--   'hybrid'    = import existing + continue syncing new items
ALTER TABLE data.source_connections
ADD COLUMN IF NOT EXISTS sync_strategy TEXT DEFAULT 'ongoing';

-- Add constraint for valid sync_strategy values
ALTER TABLE data.source_connections
ADD CONSTRAINT source_connections_sync_strategy_check
CHECK (sync_strategy IS NULL OR sync_strategy IN ('migration', 'ongoing', 'hybrid'));

-- Add comment explaining the column
COMMENT ON COLUMN data.source_connections.sync_strategy IS 'Sync strategy: migration (one-time import), ongoing (continuous sync), or hybrid (both)';
COMMENT ON COLUMN data.praxis_aspiration.target_date IS 'Specific target date for aspiration (vs relative target_timeframe)';
COMMENT ON COLUMN data.praxis_aspiration.source_provider IS 'Source of aspiration: internal (created in app), or external provider name';
