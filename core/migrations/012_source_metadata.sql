-- Add metadata column to source_connections for storing source-specific data
-- This is used by Plaid and other sources that need to store additional configuration

ALTER TABLE data.source_connections
ADD COLUMN IF NOT EXISTS metadata JSONB;

-- Add comment explaining the column
COMMENT ON COLUMN data.source_connections.metadata IS 'Source-specific metadata (e.g., Plaid item_id, access_token, institution info)';

-- Update auth_type constraint to include 'plaid'
ALTER TABLE data.source_connections DROP CONSTRAINT IF EXISTS source_connections_auth_type_check;
ALTER TABLE data.source_connections ADD CONSTRAINT source_connections_auth_type_check
  CHECK (auth_type IN ('oauth2', 'device', 'api_key', 'none', 'plaid'));
