-- Add onboarding_step column to track progress through onboarding wizard
-- This allows users to resume onboarding from where they left off

ALTER TABLE data.user_profile
ADD COLUMN IF NOT EXISTS onboarding_step INTEGER DEFAULT 0;

COMMENT ON COLUMN data.user_profile.onboarding_step IS 'Current step in onboarding wizard (0-5). NULL or 0 means not started, cleared on completion.';
