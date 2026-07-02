-- Idempotency for POST /api/pair/consume.
--
-- A device that retries consume after a lost response would otherwise find the
-- one-off token already 'consumed' and be forced to re-pair. With a
-- client-generated idempotency key, the box re-returns the SAME bearer on retry.
--
-- The bearer is stored ONLY as ciphertext (the same AES-GCM form as
-- `credentials.secrets_ciphertext`), never plaintext; rows are short-lived and
-- swept opportunistically on write.
CREATE TABLE IF NOT EXISTS app_pair_consume_idem (
    idempotency_key   text PRIMARY KEY,
    device_id         text NOT NULL,
    credential_id     text NOT NULL,
    bearer_ciphertext text NOT NULL,
    action_ids        jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at        timestamptz NOT NULL DEFAULT now()
);
