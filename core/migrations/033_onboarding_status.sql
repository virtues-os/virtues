-- Migrate onboarding_status from old wizard values to simple enum
-- Old: 'welcome', 'profile', 'places', 'tools', 'complete'
-- New: 'new', 'active', 'complete'

ALTER TABLE app_user_profile ADD COLUMN _onboarding_tmp TEXT NOT NULL DEFAULT 'new'
    CHECK (_onboarding_tmp IN ('new', 'active', 'complete'));

UPDATE app_user_profile SET _onboarding_tmp = CASE
    WHEN onboarding_status = 'complete' THEN 'complete'
    WHEN onboarding_status = 'welcome' THEN 'new'
    ELSE 'active'
END;

ALTER TABLE app_user_profile DROP COLUMN onboarding_status;

ALTER TABLE app_user_profile RENAME COLUMN _onboarding_tmp TO onboarding_status;
