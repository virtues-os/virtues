-- 0036_add_profile_home_city
--
-- The city the person names as home in Setup ("Where's home?"). Setup has
-- always sent it; there was no column, so the update dropped it without a
-- word and the assistant never knew where home was. A name as they chose
-- it, not a place: `home_place_id` points at a real address and anchors
-- visits, which a city centroid must never do.
ALTER TABLE app_user_profile ADD COLUMN IF NOT EXISTS home_city text;
