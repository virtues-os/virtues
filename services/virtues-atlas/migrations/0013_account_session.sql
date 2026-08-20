-- Account sessions for the APP.
--
-- Deliberately the smallest thing that works, because the app needs a session
-- for exactly one window: before a box is linked, to pay in-app and to vouch
-- for a link without sending anyone to a browser. Everything after that — usage,
-- wallet, billing portal — the app asks its BOX, and the box asks atlas with its
-- own api key. The box is already an authenticated client; a user session is not
-- a second way to reach the same data.
--
-- Two tables, no new personal data: the email is already in `customers` because
-- Stripe requires it.
--
-- Why a 6-digit code and not a magic link: a link opens a browser, which is the
-- exact hop this exists to delete. A code is typed into the app and the user
-- never leaves it.
--
-- Why opaque tokens and not JWT: instant revocation (lost device → kill the
-- session) matters at this scale; statelessness does not. Atlas has a database.

-- One-time login codes. The plaintext code only ever exists in the email.
CREATE TABLE IF NOT EXISTS login_code (
    code_hash   bytea PRIMARY KEY,       -- SHA-256 of the 6-digit code + email
    email       text        NOT NULL,
    attempts    int         NOT NULL DEFAULT 0,  -- guess counter; burn at 5
    consumed_at timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS login_code_email_idx   ON login_code (email, created_at DESC);
CREATE INDEX IF NOT EXISTS login_code_expires_idx ON login_code (expires_at);

-- Sessions. Long-lived on purpose (a phone should not re-authenticate monthly),
-- and revocable — which is the whole reason these are rows rather than JWTs.
CREATE TABLE IF NOT EXISTS account_session (
    token_hash   bytea PRIMARY KEY,      -- SHA-256 of a 256-bit random token
    email        text        NOT NULL,
    -- Nullable: someone can hold a session BEFORE they have paid for anything.
    -- That is the whole point — sign in, then buy, then link.
    stripe_customer_id text,
    user_agent   text,                   -- for the account page's device list
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    revoked_at   timestamptz,
    expires_at   timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS account_session_email_idx   ON account_session (email);
CREATE INDEX IF NOT EXISTS account_session_expires_idx ON account_session (expires_at);
