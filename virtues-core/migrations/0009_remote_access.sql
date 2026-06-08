-- Remote access (WS-2): box identity secrets.
--
-- Singleton secrets for THIS server, distinct from the per-device `credentials`
-- table: the per-server CA, the box's own WireGuard keypair, the rendezvous
-- identity. Sealed at rest with the vault master key (VIRTUES_ENCRYPTION_KEY),
-- same as credential secrets.
--
-- Kept OUT of `credentials` on purpose: many readers there do
-- `WHERE status = 'active'` (the connected-accounts UI, the refresh cron,
-- template reconcile), and box identity is neither a connected account nor
-- refreshable — it would only pollute those paths.
CREATE TABLE box_secrets (
    -- Logical name of the secret, e.g. 'wg_ca', 'wg_server_keypair',
    -- 'rendezvous_identity'.
    key                text PRIMARY KEY,

    -- AES-256-GCM(vault master key) of the secret material (e.g. a PEM private
    -- key). Same TokenEncryptor envelope as credentials.secrets_ciphertext.
    secret_ciphertext  text NOT NULL,

    -- Non-secret public parts kept in the clear for convenience (e.g. the CA
    -- cert PEM that ships in the pairing bundle, or a WG public key).
    metadata           jsonb NOT NULL DEFAULT '{}'::jsonb,

    updated_at         timestamptz NOT NULL DEFAULT now()
);
