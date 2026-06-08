-- Device-authorization link flow (RFC 8628 shape) for connecting a box to a
-- paid subscription WITHOUT the box ever holding a Stripe key.
--
-- The box starts a link and shows the user a short code + URL; the user
-- completes Stripe Checkout on any device; Atlas finalizes (mints the billing
-- token) and the box polls to retrieve it. Identity-side state only — nothing
-- here joins to virtues-api, no usage credential is stored.

CREATE TABLE device_link (
    -- SHA-256 of the high-entropy device code the box polls with. Only the
    -- hash is stored; the box holds the secret. Acts as the poll capability.
    device_code_hash   bytea PRIMARY KEY,
    -- Short human code the user types on the verification page (e.g. WXYZ-1234).
    user_code          text NOT NULL UNIQUE,
    -- 'pending' -> 'ready' (finalized) | 'expired' | 'denied'.
    status             text NOT NULL DEFAULT 'pending',
    -- The minted billing token, held only between finalization and the box's
    -- first successful poll, then cleared.
    billing_token      text,
    -- The Stripe Checkout session created for this link, reused across page
    -- refreshes so we don't spawn duplicates.
    stripe_session_id  text,
    created_at         timestamptz NOT NULL DEFAULT now(),
    expires_at         timestamptz NOT NULL
);

CREATE INDEX device_link_user_code_idx ON device_link (user_code);
CREATE INDEX device_link_expires_idx ON device_link (expires_at);
