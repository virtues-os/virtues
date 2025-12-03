-- Add crux column to user_profile
-- The "crux" is a shared ethos statement collected during onboarding - the user's vision shared with the team

ALTER TABLE data.user_profile ADD COLUMN crux TEXT;

COMMENT ON COLUMN data.user_profile.crux IS 'Shared ethos statement from onboarding - user''s vision and goals for Personal AI';
