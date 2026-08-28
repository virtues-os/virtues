-- Per-box api keys (agents/record/one-wire-plan.md, Phase 0).
--
-- `customers.api_key_hash` is ONE key per customer, and every attach rotates
-- it — so a household linking a second box silently killed the first box's
-- credential (the landmine the deferred /init/grant comment in
-- routes/account.rs documents). virtues-api's half has been per-box-ready
-- for a while (`register_device` scopes replacement by box_id); this is the
-- atlas half.
--
-- `box_key` is the authoritative key table: one row per live box credential.
-- `customers.api_key_hash` is retained and still written (mirror of the most
-- recently minted key) so an older atlas binary rolled back onto this schema
-- keeps authenticating; reads go box_key-first with a customers fallback.
CREATE TABLE IF NOT EXISTS box_key (
    api_key_hash       bytea PRIMARY KEY,
    stripe_customer_id text NOT NULL,
    -- Which box this key belongs to, as the box's self-reported iroh
    -- EndpointId from /init/start. A rotation-scoping LABEL, never an
    -- authorization input: it arrives on an unauthenticated call, so a forged
    -- value can mislabel a key but must never grant or deny anything. NULL =
    -- an older box that did not identify itself; those keep the historical
    -- whole-account rotation.
    endpoint_id        text,
    created_at         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS box_key_customer_idx ON box_key (stripe_customer_id);

-- The box that started a link, so attach can scope the key it mints. The box
-- has been SENDING this in the /init/start body all along ("atlas tolerates
-- its absence — older boxes send no body"); atlas simply never read it.
ALTER TABLE device_link ADD COLUMN IF NOT EXISTS endpoint_id text;

-- Backfill: every existing single-key customer becomes one box_key row with
-- an unknown endpoint. Their next attach re-scopes it.
INSERT INTO box_key (api_key_hash, stripe_customer_id)
SELECT api_key_hash, stripe_customer_id
FROM customers
WHERE api_key_hash IS NOT NULL
ON CONFLICT (api_key_hash) DO NOTHING;
