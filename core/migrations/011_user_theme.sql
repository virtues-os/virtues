-- Add theme preference to user profile
ALTER TABLE data.user_profile
ADD COLUMN IF NOT EXISTS theme TEXT DEFAULT 'warm';

-- Add check constraint for valid themes
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'valid_theme'
    ) THEN
        ALTER TABLE data.user_profile
        ADD CONSTRAINT valid_theme CHECK (theme IN ('warm', 'light', 'dark', 'night'));
    END IF;
END
$$;
