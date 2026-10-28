-- 0062_one_secrets_table
--
-- Every secret on the box goes in one table. `credentials` gains `kind`:
-- `source` for what a sync uses (sessions, tokens, keys), `box` for the
-- box's own keys, and later `login` for the owner's saved logins
-- (agents/plan/secrets-plan.md). `box_secrets` folds in as `kind = 'box'` and
-- is dropped.
--
-- `source_id` becomes `key`: what the secret unlocks. For a source row that is
-- still the source id; for a box row it is the name the code looks it up by
-- (the iroh secret, the link code). Every row stays sealed with the same box
-- key, so the ciphertext moves across unchanged and nothing is re-encrypted.

ALTER TABLE credentials
    ADD COLUMN kind text NOT NULL DEFAULT 'source'
        CHECK (kind IN ('source', 'box', 'login'));

ALTER TABLE credentials RENAME COLUMN source_id TO key;
ALTER INDEX idx_credentials_source RENAME TO idx_credentials_key;

-- A box key names exactly one row; `crate::box_secrets` upserts on it.
CREATE UNIQUE INDEX credentials_box_key_idx ON credentials (key) WHERE kind = 'box';

-- A box row's plaintext is the bare secret string, as it was in box_secrets.
INSERT INTO credentials (id, kind, key, name, status, secrets_ciphertext, metadata, updated_at)
SELECT 'box:' || key, 'box', key, key, 'active', secret_ciphertext, metadata, updated_at
FROM box_secrets;

DROP TABLE box_secrets;
