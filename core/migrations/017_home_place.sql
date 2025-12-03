-- Add home_place_id to user_profile for linking to entities_place
-- This replaces the separate home_street, home_city, etc. fields with a proper place entity reference

ALTER TABLE data.user_profile ADD COLUMN IF NOT EXISTS home_place_id UUID REFERENCES data.entities_place(id);

CREATE INDEX IF NOT EXISTS idx_user_profile_home_place ON data.user_profile(home_place_id) WHERE home_place_id IS NOT NULL;
