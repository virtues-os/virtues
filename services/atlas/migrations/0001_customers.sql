-- WS-7 (voucher model): atlas customer + billing schema.
--
-- This is the identity side of the wall. Atlas knows who you are and that
-- you pay. It knows NOTHING about your usage bearer — that lives only in
-- virtues-api, and the two share no column. The bridge between them is a
-- disposable voucher that neither side retains as a link.
--
-- See Virtues-API.md (the idea) and docs/entitlement.md (spec).


-- One row per Stripe customer.
--
-- `billing_token_hash`: SHA-256 of the stable credential the home server
-- presents to prove "I'm a paying customer" when fetching a monthly
-- voucher. It is NOT a usage identifier and never reaches virtues-api.
--
-- `last_voucher_issued_at`: rate-limits voucher minting (one per ~month)
-- so a customer can't mint a stack of vouchers. Customer-side state only;
-- carries no bearer link.
--
-- Caps (v3, locked 2026-06-05): customer-tunable spending limits, surfaced
-- via iOS Settings. Atlas enforces both before charging the saved card:
--   * `monthly_cap_micros`: $100 default, $100–$1000 user-settable.
--     Caps sub + top-ups combined within a calendar month.
--   * `daily_cap_micros`: $20 default, user-tunable. Daily wallet-spend
--     ceiling — enforced inside virtues-api's charge() to bound runaway
--     loops. (Atlas-side mirror so iOS Settings can present + update it
--     via a single GET/PUT pair.)
--
-- `monthly_charges_micros` + `month_reset_at`: rolling count of charges
-- this calendar month, reset at first-of-month UTC. Used to enforce the
-- monthly cap when atlas auto-tops-up a saved card.
CREATE TABLE customers (
    stripe_customer_id        text PRIMARY KEY,
    email                     text NOT NULL,
    billing_token_hash        bytea,
    last_voucher_issued_at    timestamptz,
    -- v3 caps
    monthly_cap_micros        bigint NOT NULL DEFAULT 100000000,  -- $100
    daily_cap_micros          bigint NOT NULL DEFAULT 20000000,   -- $20
    monthly_charges_micros    bigint NOT NULL DEFAULT 0,
    month_reset_at            timestamptz NOT NULL DEFAULT date_trunc('month', now() + interval '1 month'),
    -- v3 auto-top-up control
    auto_topup_enabled        boolean NOT NULL DEFAULT true,
    last_auto_topup_at        timestamptz,
    created_at                timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX customers_email_idx ON customers (email);
CREATE INDEX customers_billing_token_idx ON customers (billing_token_hash);


-- Stripe subscription state, mirrored from webhooks. Single $29/mo plan,
-- so no sku/tier columns — everyone is the same.
CREATE TABLE subscriptions (
    stripe_subscription_id   text PRIMARY KEY,
    stripe_customer_id       text NOT NULL REFERENCES customers (stripe_customer_id) ON DELETE CASCADE,
    status                   text NOT NULL,           -- 'active' | 'past_due' | 'canceled' | 'refunded'
    current_period_end       timestamptz,
    created_at               timestamptz NOT NULL DEFAULT now(),
    updated_at               timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX subscriptions_customer_idx ON subscriptions (stripe_customer_id);


-- Webhook idempotency: Stripe can deliver the same event twice. Dedup
-- ledger, not an events log.
CREATE TABLE stripe_webhook_events (
    stripe_event_id    text PRIMARY KEY,
    event_type         text NOT NULL,
    processed_at       timestamptz NOT NULL DEFAULT now()
);


CREATE OR REPLACE FUNCTION bump_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER subscriptions_bump_updated_at
    BEFORE UPDATE ON subscriptions
    FOR EACH ROW EXECUTE FUNCTION bump_updated_at();
