-- Migration: Track whether user has completed axiology discovery via chat
-- This flag determines whether to use the onboarding agent or regular agent

ALTER TABLE data.user_profile
ADD COLUMN IF NOT EXISTS axiology_complete BOOLEAN DEFAULT FALSE;

-- Comment for clarity
COMMENT ON COLUMN data.user_profile.axiology_complete IS 'Whether the user has completed axiology discovery through chat conversation with the onboarding agent';
