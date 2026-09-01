-- Accounts decoupled from Stripe (open-relay-plan §Work 1b).
--
-- `customers.stripe_customer_id` has been the PRIMARY KEY of identity: no
-- Stripe customer, no account_id, no box_keys, nothing to attach a box to.
-- That forced every free sign-in toward Stripe (or a 402), and it is why the
-- linking paths carried entitlement gates that had nothing to do with money —
-- the schema simply could not represent "an account that hasn't paid".
--
-- `accounts` inverts it: account_id is minted at first sign-in from nothing
-- but the email, and Stripe becomes an attribute attached at first checkout.
-- Stripe learns about exactly the people who pay it. `customers` stays as the
-- billing-side table (caps, top-ups, webhook mirror) — it is no longer the
-- identity root.

CREATE TABLE IF NOT EXISTS accounts (
    -- `acct_<32hex>`, same shape/minting as customers.account_id (0008) —
    -- shared with virtues-api as the wallet key, never a Stripe id or email.
    account_id         text PRIMARY KEY,
    email              text NOT NULL UNIQUE,
    -- Attached at first checkout; NULL = has never paid. Deliberately not a
    -- FK: identity must not depend on billing rows existing.
    stripe_customer_id text UNIQUE,
    created_at         timestamptz NOT NULL DEFAULT now()
);

-- Backfill: one account per distinct email. Duplicate emails across
-- customers are real (a second checkout minted a second Stripe customer);
-- prefer the one holding an active subscription, then the newest. The
-- account_id is REUSED from customers so existing wallets keep their key.
INSERT INTO accounts (account_id, email, stripe_customer_id, created_at)
SELECT DISTINCT ON (lower(c.email))
       c.account_id, lower(c.email), c.stripe_customer_id, c.created_at
FROM customers c
LEFT JOIN subscriptions s
  ON s.stripe_customer_id = c.stripe_customer_id AND s.status = 'active'
ORDER BY lower(c.email), (s.stripe_subscription_id IS NULL), c.created_at DESC
ON CONFLICT DO NOTHING;

-- The unique index from 0008 assumed one customer row per account. Under
-- 0017 the account is email-keyed and a re-subscribe legitimately mints a
-- SECOND Stripe customer carrying the SAME account_id — with the unique
-- index in place, that insert dies and checkout finalize fails forever
-- (payment captured, nothing provisioned). Plain index keeps the lookups.
-- Dropped BEFORE the alignment below, which itself creates exactly such
-- same-account sibling rows.
DROP INDEX IF EXISTS customers_account_id_idx;
CREATE INDEX IF NOT EXISTS customers_account_idx ON customers (account_id);

-- ONE account_id per email, everywhere. Align every customers row (including
-- the DISTINCT ON losers above) to the account that won its email, so the
-- money loop cannot fork: webhooks credit `customers.account_id` while
-- linking registers boxes under `accounts.account_id`, and any divergence
-- sends a subscription's monthly credit to a wallet no box reads. A box that
-- was keyed under a loser customer's old id keeps spending its old wallet
-- until its next re-link (self-healing, beta-scale cohort ≈ 0); renewals fund
-- the surviving id from the moment this runs.
UPDATE customers c
   SET account_id = a.account_id
  FROM accounts a
 WHERE a.email = lower(c.email)
   AND c.account_id <> a.account_id;

-- box_key gains its true owner, and rotation scope moves with it. Backfilled
-- AFTER the alignment above, so every key lands on the email's surviving
-- account and an account-scoped rotation retires every historical key for
-- that owner. stripe_customer_id relaxes to nullable (a free account's box
-- has no customer) but keeps being written when known, so a rolled-back
-- binary keeps authenticating. account_id stays NULLABLE for the same
-- rollback room: the pre-0017 binary's mint inserts without it, and a NOT
-- NULL here would take linking down under exactly the rollback it protects.
ALTER TABLE box_key ADD COLUMN IF NOT EXISTS account_id text;
UPDATE box_key bk
   SET account_id = c.account_id
  FROM customers c
 WHERE c.stripe_customer_id = bk.stripe_customer_id
   AND bk.account_id IS NULL;
ALTER TABLE box_key ALTER COLUMN stripe_customer_id DROP NOT NULL;
CREATE INDEX IF NOT EXISTS box_key_account_idx ON box_key (account_id);

-- Grants bind to the account, not the customer. stripe_customer_id (0016)
-- keeps being written when the session has one, for one release of rollback
-- room; the redeem path reads account_id first.
ALTER TABLE device_link ADD COLUMN IF NOT EXISTS account_id text;

-- The magic-link flow (`virtues link` on the box console) also becomes
-- account-first: an email with no Stripe customer used to dead-end at
-- "no_account" — the same lock as the linking 402s, one more organ. The
-- account is resolved at VERIFY (the click is the proof of the address;
-- minting at send would let one device_code create junk accounts for any
-- typed email), so the attempt row only needs its customer relaxed.
ALTER TABLE login_attempt ALTER COLUMN customer_id DROP NOT NULL;
