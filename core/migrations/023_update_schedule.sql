-- Add update_check_hour to user_profile for configurable automatic update times
-- Default is 8 (8:00 UTC = 3:00 AM Central Time)
-- The systemd timer on the VPS reads this value to schedule daily update checks

ALTER TABLE data.user_profile ADD COLUMN update_check_hour INTEGER DEFAULT 8;

-- Constraint to ensure valid hour (0-23)
ALTER TABLE data.user_profile ADD CONSTRAINT user_profile_update_check_hour_range
    CHECK (update_check_hour >= 0 AND update_check_hour <= 23);

COMMENT ON COLUMN data.user_profile.update_check_hour IS 'Hour (0-23 UTC) when the system checks for updates. Default 8 (3 AM Central). User can configure via settings.';
