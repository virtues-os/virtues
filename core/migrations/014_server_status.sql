-- Migration: Add server_status for Tollbooth hydration
-- Replaces the onboarding wizard redirect pattern with a simpler
-- "provisioning" -> "ready" state machine.
--
-- States:
--   'provisioning' - Container just started, waiting for Tollbooth hydration
--   'migrating'    - Restoring from cold storage (zombie wake-up)  
--   'ready'        - Normal operation
--
-- The onboarding_status column is kept for backward compatibility but
-- no longer controls access to the app.

-- Add server_status column with default 'provisioning'
ALTER TABLE app_user_profile 
ADD COLUMN server_status TEXT NOT NULL DEFAULT 'provisioning' 
CHECK (server_status IN ('provisioning', 'migrating', 'ready'));

-- Migrate existing users: if they completed onboarding, they're ready
UPDATE app_user_profile 
SET server_status = 'ready' 
WHERE onboarding_status = 'complete';

-- For users mid-onboarding, also mark as ready (they'll use Getting Started)
UPDATE app_user_profile 
SET server_status = 'ready',
    onboarding_status = 'complete'
WHERE onboarding_status != 'complete' 
  AND server_status = 'provisioning';

-- Index for quick lookups
CREATE INDEX IF NOT EXISTS idx_app_user_profile_server_status 
ON app_user_profile(server_status);
