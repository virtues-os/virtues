-- Login attempts for the "[1] Log in to existing Virtues account" flow.
--
-- The box mints a device_code (existing /init/login_start), prints a URL
-- to the user. User opens the URL, enters their email. Atlas looks up
-- the matching Stripe customer; if found, sends a magic link via Resend.
-- User clicks the link; we mark the device_link associated with that
-- attempt as ready with a billing_token for the verified customer.
--
-- Tokens are stored as SHA-256 hashes — the raw token only lives in
-- the URL we email to the user.

CREATE TABLE login_attempt (
    -- SHA-256 of the magic-link token. The raw token is only emailed
    -- to the user; we never log it or store it.
    token_hash       bytea PRIMARY KEY,

    -- Email the link was sent to. Plaintext because we already see it
    -- (had to look up the Stripe customer); no privacy gain in hashing.
    email            text NOT NULL,

    -- Stripe customer ID we resolved at /init/login time. We freeze it
    -- here so a customer-record change between send + click doesn't
    -- attach the box to the wrong subscription.
    customer_id      text NOT NULL,

    -- The device_link this attempt is bound to. On verify, we flip the
    -- corresponding device_link to status='ready' with a billing_token.
    device_code_hash bytea NOT NULL REFERENCES device_link(device_code_hash) ON DELETE CASCADE,

    -- 'pending' → 'used' (verified, link clicked) | 'expired' | 'invalidated'.
    status           text NOT NULL DEFAULT 'pending',

    created_at       timestamptz NOT NULL DEFAULT now(),
    expires_at       timestamptz NOT NULL,
    used_at          timestamptz
);

-- Verify lookups happen by token_hash (PK); no other index needed for
-- correctness. Add expiry sweep index so the cleanup job is fast.
CREATE INDEX login_attempt_expires_idx ON login_attempt (expires_at);

-- For the rate limiter: how many attempts have we sent to this email
-- in the last hour? Plain B-tree on (email, created_at) covers it.
CREATE INDEX login_attempt_email_recency_idx ON login_attempt (email, created_at DESC);
