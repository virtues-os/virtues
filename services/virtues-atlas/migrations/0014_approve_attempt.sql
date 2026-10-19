-- Per-account attempt budget for POST /init/approve (the app's inline sign-in).
--
-- Approve attaches a box's in-flight link keyed on its short user_code. The
-- code space is large (32^8) and the live-pending set is tiny, so blind
-- guessing is impractical on the numbers — but the endpoint still needs the
-- same defense the file's other doors have: /init/done binds the session to a
-- Stripe session id, /account/login caps sends per hour + burns attempts.
-- Without a budget here, an entitled session can grind the endpoint as a
-- 404-vs-200 oracle for which codes are live, invisibly. This table makes the
-- grind bounded AND loud (approve also logs misses at warn).
--
-- Keyed on the authenticated account's email — the same handle the session
-- carries — so the cap follows the attacker's identity, not their IP. One row
-- per approve call; swept by expiry like login_code.
CREATE TABLE IF NOT EXISTS approve_attempt (
    email      text        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- The rate-limit read is "how many approve calls for this email in the last
-- hour?" — a plain B-tree on (email, created_at) covers it, and the same index
-- serves the hourly cleanup sweep.
CREATE INDEX IF NOT EXISTS approve_attempt_email_recency_idx
    ON approve_attempt (email, created_at DESC);
