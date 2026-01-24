-- Owner Email: Seed and Drift Pattern
-- Environment variable seeds on first boot, database becomes source of truth
-- Atlas can update via webhook without container restart

-- Add owner_email to user profile singleton
ALTER TABLE app_user_profile ADD COLUMN owner_email TEXT;

-- Index for lookup (though singleton, makes intent clear)
CREATE INDEX IF NOT EXISTS idx_app_user_profile_owner_email ON app_user_profile(owner_email);
