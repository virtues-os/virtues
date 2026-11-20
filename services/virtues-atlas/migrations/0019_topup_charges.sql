-- One row per off-session top-up charge that atlas has credited.
--
-- A top-up charges the card, then credits the wallet. When the box retries a
-- top-up whose first attempt timed out after Stripe had already charged,
-- atlas sends the same Stripe idempotency key, so Stripe returns the first
-- PaymentIntent instead of charging again. This table is what tells atlas
-- whether that PaymentIntent was already credited, so the retry credits it
-- once and a repeat of a finished top-up credits nothing.
CREATE TABLE IF NOT EXISTS topup_charges (
    stripe_payment_intent_id text PRIMARY KEY,
    stripe_customer_id       text NOT NULL,
    amount_micros            bigint NOT NULL,
    created_at               timestamptz NOT NULL DEFAULT now()
);
