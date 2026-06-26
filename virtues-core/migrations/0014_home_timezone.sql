-- Timezone model: rename profile.timezone → home_timezone.
--
-- `home_timezone` is the timezone of the box's physical location (the owner's
-- home), read from the server's own system clock at onboarding. It is a stable
-- anchor + fallback floor — it does NOT track where the owner currently is.
-- The per-day "where the owner was" timezone lives on wiki_days.start_timezone.
-- See docs/timezone-model.md.

ALTER TABLE app_user_profile RENAME COLUMN timezone TO home_timezone;
