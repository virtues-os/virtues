-- Persistent pair code for App Store review boxes.
--
-- A third kind joins 'oneoff' and 'standing' in app_pair_token:
--   • 'review' — a code that does NOT rotate and does NOT expire. Multi-use,
--                like 'standing', because `claim_pair_token` only consumes
--                rows where kind = 'oneoff'.
--
-- Why this exists: pairing is reachability-limited to the LAN (the box has no
-- public inbound HTTP port), so an App Store reviewer — who has no box and no
-- cable — cannot pair at all. The answer is a throwaway demo box exposed over
-- HTTPS with seeded synthetic data, plus a code that is still valid whenever
-- the reviewer happens to open the app, across review rounds days apart. Both
-- existing kinds are far too short-lived for that: 5–30 min for 'oneoff',
-- ~20 min for 'standing'.
--
-- SAFETY: this widens a CHECK constraint only. It creates no row. A 'review'
-- row exists only where VIRTUES_REVIEW_PAIR_CODE is set in the environment
-- (see `api::pair::ensure_review_code`), which is never true on a customer
-- box. That env gate is the control — the migration is inert without it.
--
-- A 'review' code is a permanent remote-pairing credential for whatever box
-- holds it, so it belongs ONLY on a disposable box carrying synthetic data
-- (`virtues seed`), never on a box holding a real person's life.

ALTER TABLE app_pair_token
    DROP CONSTRAINT IF EXISTS app_pair_token_kind_check;

ALTER TABLE app_pair_token
    ADD CONSTRAINT app_pair_token_kind_check
        CHECK (kind IN ('oneoff', 'standing', 'review'));
