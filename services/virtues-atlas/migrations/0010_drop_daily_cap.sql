-- Cursor-style billing: drop the daily cap entirely. The monthly cap is now
-- the only user-settable spend ceiling (it bounds top-ups within a calendar
-- month). Raise its default from $100 to $200, and migrate customers still
-- sitting on the old $100 default up to the new $200 default.
ALTER TABLE customers DROP COLUMN IF EXISTS daily_cap_micros;
ALTER TABLE customers ALTER COLUMN monthly_cap_micros SET DEFAULT 200000000; -- $200
UPDATE customers SET monthly_cap_micros = 200000000 WHERE monthly_cap_micros = 100000000;
