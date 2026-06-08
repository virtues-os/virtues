-- virtues-api entitlement schema (v3, locked 2026-06-05).
--
-- This service knows NOTHING about people. No customer_id, no email, no
-- shared key with Atlas. It knows only that some anonymous bearer has a
-- wallet balance and an expiry. The link between a person and a bearer
-- exists solely on the user's own home server.
--
-- Single $20/mo plan. Single wallet (no OS/chat split — dropped in v3 in
-- favor of auto-top-up when wallet hits zero). 20% universal markup is
-- applied inside `charge()` before debit (see entitlement.rs).


-- One row per bearer. Authentication: raw bearer in `Authorization`
-- header → SHA-256 → lookup. Access gated by `expires_at`. Vouchers (see
-- `voucher.rs`) refill `wallet_micros` on redeem. There is no `active`
-- flag and no `tier` (single $20/mo plan + top-ups).
CREATE TABLE entitlements (
    bearer_hash               bytea PRIMARY KEY,

    -- Single wallet (micros USD). All charged calls debit this. Sub
    -- renewal sets to $15 (overwrite, monthly cohort-aligned). Top-ups
    -- ADD to it (accumulate). 402 when zero — box catches and triggers
    -- auto-top-up flow.
    wallet_micros             bigint NOT NULL DEFAULT 0,

    -- Daily spend ceiling — runaway-loop + bearer-leak circuit breaker.
    -- Default $20/day (user-tunable via atlas customers.daily_cap_micros).
    -- Reset lazily to 0 on the first call after `today_reset_at`.
    today_spent_micros        bigint NOT NULL DEFAULT 0,
    today_reset_at            timestamptz NOT NULL,

    -- Wallet expiry (cohort-aligned 1st of month UTC, defense-in-depth
    -- against timing fingerprinting). A bearer past expiry is dead — the
    -- device must redeem a fresh voucher (sub renewal mints one).
    expires_at                timestamptz NOT NULL,

    created_at                timestamptz NOT NULL DEFAULT now(),
    updated_at                timestamptz NOT NULL DEFAULT now()
);


-- Vouchers: disposable bridge between Atlas (billing) and this gate.
-- Atlas mints a code, registers it here (carrying ONLY the amount it's
-- worth — no customer, no bearer), and hands the code to the device. The
-- device redeems it onto its bearer. On redemption the row is updated
-- with `redeemed_at` (hour-bucketed for timing-correlation resistance),
-- then deleted by the sweeper 24h later. That discard is what keeps the
-- customer↔bearer chain from ever existing in one place.
--
-- Amounts (defaults, env-tunable on atlas):
--   * Sub renewal: $15 (overwrite wallet — fresh monthly allocation)
--   * Manual top-up: $10–$50 user choice (added to wallet)
--   * Auto-top-up:   $10 fixed                  (added to wallet)
CREATE TABLE vouchers (
    voucher_code_hash    bytea PRIMARY KEY,        -- SHA-256 of raw code
    amount_micros        bigint NOT NULL,          -- single amount, no pool split
    is_renewal           boolean NOT NULL DEFAULT false,  -- true=overwrite wallet, false=add to it
    voucher_expires_at   timestamptz NOT NULL,     -- unredeemed self-expiry (~7d)
    redeemed_at          timestamptz               -- hour-bucketed; row deleted 24h later
);

CREATE INDEX vouchers_expires_idx ON vouchers (voucher_expires_at);
CREATE INDEX vouchers_redeemed_idx ON vouchers (redeemed_at) WHERE redeemed_at IS NOT NULL;


-- Behavioral abuse blocklist. In-memory primary; this table is a
-- restart-snapshot. TTL'd. Keyed on the anonymous bearer — Atlas/customer
-- is never involved (and couldn't be: this service has no customer link).
CREATE TABLE blocklist (
    bearer_hash      bytea PRIMARY KEY,
    reason_code      smallint NOT NULL,
    blocked_at       timestamptz NOT NULL DEFAULT now(),
    expires_at       timestamptz NOT NULL
);

CREATE INDEX blocklist_expires_at_idx ON blocklist (expires_at);


-- Auto-bump updated_at on entitlements row changes.
CREATE OR REPLACE FUNCTION bump_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER entitlements_bump_updated_at
    BEFORE UPDATE ON entitlements
    FOR EACH ROW EXECUTE FUNCTION bump_updated_at();
