-- Collapse to the linked prepaid-ledger model (v1).
--
-- The double-blind (atlas identity <-> single-use vouchers <-> bearer-keyed
-- wallet) is dropped. This service now keys the wallet by an opaque
-- `account_id` that atlas assigns per customer; rotatable device API keys map
-- to it via `device_keys`; and an append-only `ledger` is the source of truth
-- for every money movement (balance is a fast projection on `accounts`).
--
-- Privacy posture: the proxy still knows NOTHING about people — `account_id`
-- is an opaque random string, never a Stripe id or email. What collapses is
-- that account_id is shared with atlas (which holds identity), so identity and
-- usage are linkable by joining the two sides. Content still never leaves the
-- box; we store token counts + costs, never prompts. Blind-unlinkability
-- (RFC 9474) remains the documented v2.
--
-- Clean cutover: pre-launch userbase is ~one box, so we drop the old tables
-- rather than migrate rows. The box re-links once (mints an api_key, registers
-- the device, sets the wallet from the active subscription).

DROP TRIGGER IF EXISTS entitlements_bump_updated_at ON entitlements;
DROP TABLE IF EXISTS vouchers;
DROP TABLE IF EXISTS entitlements;

-- The wallet, keyed by the stable account. This is the fast gate + a pure
-- projection of `ledger` (balance_micros == SUM(ledger.micros)); rebuildable
-- at any time. The device key is a separate, rotatable pointer (device_keys),
-- so losing/rotating a key never touches the balance.
CREATE TABLE accounts (
    account_id          text PRIMARY KEY,

    -- Spendable balance (micros USD). Renewal SETs it to the monthly
    -- allotment ($20); top-ups ADD; charges debit. 402 when it can't cover
    -- the next call.
    balance_micros      bigint NOT NULL DEFAULT 0,

    -- Daily spend ceiling — runaway-loop + key-leak circuit breaker. Lazily
    -- reset to 0 on the first call after `today_reset_at`. Carried from the
    -- customer's atlas-side `customers.daily_cap_micros`.
    today_spent_micros  bigint NOT NULL DEFAULT 0,
    today_reset_at      timestamptz NOT NULL,
    daily_cap_micros    bigint NOT NULL DEFAULT 20000000,  -- $20/day

    -- Wallet expiry (cohort-aligned 1st of month UTC). Renewal bumps it;
    -- "use it or lose it" — a lapsed subscription's leftover auto-expires.
    expires_at          timestamptz NOT NULL,

    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

-- Rotatable device credentials. The box sends `Authorization: Bearer <api_key>`;
-- we SHA-256 it and resolve the account. Recovery/rotation = replace the row
-- for an account with a new key hash; the balance is untouched.
CREATE TABLE device_keys (
    api_key_hash    bytea PRIMARY KEY,            -- SHA-256 of the raw api_key
    account_id      text NOT NULL REFERENCES accounts (account_id) ON DELETE CASCADE,
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX device_keys_account_idx ON device_keys (account_id);

-- Append-only journal of every money movement. The single source of truth;
-- `accounts.balance_micros` is a cache of SUM(micros). Corrections/refunds are
-- compensating rows, never destructive edits.
--   kind: 'grant'  (subscription renewal, +)
--         'topup'  (card top-up, +)
--         'charge' (per-call debit, -)
--         'refund' (failed-call credit, +)
--         'adjust' (manual correction, +/-)
CREATE TABLE ledger (
    id          bigserial PRIMARY KEY,
    -- FK with CASCADE so sweeping an expired account takes its ledger with it
    -- (no orphan rows); the `balance == SUM(ledger)` invariant holds for every
    -- live account.
    account_id  text NOT NULL REFERENCES accounts (account_id) ON DELETE CASCADE,
    ts          timestamptz NOT NULL DEFAULT now(),
    micros      bigint NOT NULL,             -- +credit / -charge
    kind        text NOT NULL,
    real_micros bigint,                       -- pre-markup cost (charges only)
    ref         text                          -- stripe id / call id / note
);
CREATE INDEX ledger_account_ts_idx ON ledger (account_id, ts);

-- Auto-bump updated_at on account changes (reuses bump_updated_at() from 0001).
CREATE TRIGGER accounts_bump_updated_at
    BEFORE UPDATE ON accounts
    FOR EACH ROW EXECUTE FUNCTION bump_updated_at();

-- The blocklist now keys on the api_key hash (the credential it rate-limits),
-- not a bearer. Rename the column to match.
ALTER TABLE blocklist RENAME COLUMN bearer_hash TO key_hash;
