-- 0063_secrets_env_kind
--
-- A fourth kind of secret: `env`, a named key an applet declares it needs
-- (`OPENAI_API_KEY`), injected into that applet's runs as an environment
-- variable (agents/plan/secrets-plan.md). Like `box`, an `env` row is one
-- value per name, so `crate::secrets` upserts on `(kind, key)` for both.

ALTER TABLE credentials DROP CONSTRAINT credentials_kind_check;
ALTER TABLE credentials
    ADD CONSTRAINT credentials_kind_check CHECK (kind IN ('source', 'box', 'env', 'login'));

DROP INDEX credentials_box_key_idx;
CREATE UNIQUE INDEX credentials_singleton_key_idx ON credentials (kind, key)
    WHERE kind IN ('box', 'env');
