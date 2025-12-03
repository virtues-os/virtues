-- Add granular onboarding completion tracking
-- Replaces the single onboarding_step integer with individual boolean flags

-- Profile completion (name, occupation, etc.)
ALTER TABLE data.user_profile
ADD COLUMN IF NOT EXISTS onboarding_profile_complete BOOLEAN DEFAULT false;

-- Places completion (added at least one place or explicitly skipped)
ALTER TABLE data.user_profile
ADD COLUMN IF NOT EXISTS onboarding_places_complete BOOLEAN DEFAULT false;

-- Tools completion (connected at least one source or explicitly skipped)
ALTER TABLE data.user_profile
ADD COLUMN IF NOT EXISTS onboarding_tools_complete BOOLEAN DEFAULT false;

-- Note: axiology_complete already exists from migration 016

-- Comments
COMMENT ON COLUMN data.user_profile.onboarding_profile_complete IS 'Whether the user has completed profile setup (name required)';
COMMENT ON COLUMN data.user_profile.onboarding_places_complete IS 'Whether the user has added places or skipped this step';
COMMENT ON COLUMN data.user_profile.onboarding_tools_complete IS 'Whether the user has connected tools or skipped this step';

-- Derive initial values from existing data for existing users
UPDATE data.user_profile
SET onboarding_profile_complete = (preferred_name IS NOT NULL AND preferred_name != '')
WHERE onboarding_profile_complete = false;
