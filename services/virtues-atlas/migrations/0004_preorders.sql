-- Pre-order deposits.
--
-- A pre-order is a one-time, fully-refundable deposit collected via Stripe
-- Checkout (mode=payment) and credited toward the hardware at fulfillment.
-- This is a separate lifecycle from subscriptions: no billing token, no
-- voucher — just a record that someone reserved a unit. The remaining balance
-- is collected later through a separate "finish your order" flow.
--
-- Recorded by the webhook on `checkout.session.completed` where
-- `metadata.type = 'preorder_deposit'`. Idempotent on the session id (the
-- webhook-event ledger also dedups deliveries).

CREATE TABLE preorders (
    stripe_session_id       text PRIMARY KEY,
    stripe_payment_intent   text,
    email                   text,
    amount_total            bigint,            -- smallest currency unit (e.g. cents)
    currency                text,
    status                  text NOT NULL DEFAULT 'deposit_paid',  -- 'deposit_paid' | 'refunded' | 'fulfilled'
    created_at              timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX preorders_email_idx ON preorders (email);
CREATE INDEX preorders_payment_intent_idx ON preorders (stripe_payment_intent);
