-- Anti-replay for Stripe Checkout Session ids.
--
-- A paid `cs_*` id can be observed in browser URLs, server logs, and referrers;
-- without a uniqueness guard, `finalize_paid_session` would mint a fresh
-- billing token on every replay (and rotate the legitimate customer's token
-- via the customers UPSERT, silently DoS-ing them). This table makes finalize
-- a one-shot per session id.
--
-- Insert at the top of finalize_paid_session with ON CONFLICT DO NOTHING;
-- rows_affected==0 means the session was already consumed and the request
-- must be rejected.

CREATE TABLE claimed_sessions (
    stripe_session_id  text PRIMARY KEY,
    claimed_at         timestamptz NOT NULL DEFAULT now()
);
