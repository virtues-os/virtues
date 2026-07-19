-- 0022 — Auto-top-up RATE guard (velocity breaker).
--
-- The existing breaker (auto_topup_failures_24h) only trips on FAILED charges.
-- The runaway that drained the wallet did the opposite: it SUCCEEDED in
-- charging repeatedly (transcription loop → wallet empty → auto-refill →
-- repeat → "two $10 refuel emails overnight"). Nothing capped the velocity of
-- *successful* refills. These columns track auto-top-ups within a rolling
-- window so a fast succession trips `auto_topup_enabled = FALSE` (the same gate
-- the failure breaker uses), forcing the user to intervene before the card is
-- charged unboundedly. The monthly cap still bounds total spend; this bounds
-- the rate.
ALTER TABLE app_user_profile
    ADD COLUMN IF NOT EXISTS auto_topup_count_window INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS auto_topup_window_start TIMESTAMPTZ;
