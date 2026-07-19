-- Box-side state for the "link a device" flow (fully-remote enrollment).
--
-- A voucher (already-paired device) starts a session → the box mints a one-time
-- code C, stores H(C) here, opens an atlas rendezvous, and later (on approve)
-- enrolls the new device (records its allowlisted iroh EndpointId). The new
-- device redeems over iroh with C to fetch its ingest action map. One-time;
-- swept on expiry. No bearer changes hands — auth is the allowlisted key.
CREATE TABLE IF NOT EXISTS app_link_session (
    code_hash          text PRIMARY KEY,          -- SHA-256(C), hex
    status             text NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending', 'approved', 'redeemed')),
    -- Filled at approve time from the atlas session (the new device's EndpointId).
    device_endpoint_id text,
    -- The device row enrolled at approve; redeem returns its action map.
    device_id          text,
    action_ids         jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at         timestamptz NOT NULL DEFAULT now(),
    expires_at         timestamptz NOT NULL
);
