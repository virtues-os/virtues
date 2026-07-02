-- Box-side state for the "link a device" flow (fully-remote enrollment).
--
-- A voucher (already-paired device) starts a session → the box mints a one-time
-- code C, stores H(C) here, opens an atlas rendezvous, and later (on approve)
-- runs enroll_peer_core and STASHES the minted bearer (ciphertext-only) here
-- until the new device redeems it over iroh with C. One-time; swept on expiry.
CREATE TABLE IF NOT EXISTS app_link_session (
    code_hash          text PRIMARY KEY,          -- SHA-256(C), hex
    status             text NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending', 'approved', 'redeemed')),
    -- Filled at approve time from the atlas session (the new device's EndpointId).
    device_endpoint_id text,
    -- Minted at approve, delivered at redeem. Ciphertext only (never plaintext).
    bearer_ciphertext  text,
    credential_id      text,
    action_ids         jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at         timestamptz NOT NULL DEFAULT now(),
    expires_at         timestamptz NOT NULL
);
