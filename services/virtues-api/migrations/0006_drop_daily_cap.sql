-- Cursor-style billing: the only spend ceiling is the user-settable MONTHLY
-- top-up cap (enforced atlas-side on top-up). The per-day wall is gone, so the
-- daily-cap bookkeeping columns on `accounts` are dead. Drop them.
--
-- `charge()`/`settle()` no longer read or write any of these; the pre-flight
-- budget gate now only checks balance + expiry.
ALTER TABLE accounts DROP COLUMN IF EXISTS today_spent_micros;
ALTER TABLE accounts DROP COLUMN IF EXISTS today_reset_at;
ALTER TABLE accounts DROP COLUMN IF EXISTS daily_cap_micros;
