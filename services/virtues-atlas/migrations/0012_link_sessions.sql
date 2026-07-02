-- Rendezvous for the "link a device" flow. atlas is a BLIND coordinator: it
-- holds only a code HASH, the box's public reach, and the new device's public
-- EndpointId + a MAC. It never sees the linking code plaintext or the bearer
-- (the bearer flows box→device over iroh at redeem). Mirrors device_link (0002).
CREATE TABLE IF NOT EXISTS link_session (
    code_hash          text PRIMARY KEY,   -- SHA-256(C), hex — box + device both send this
    account_id         text NOT NULL,      -- owning account (from resolve_active_customer)
    box_node_id        text NOT NULL,      -- box reach (public), handed to the new device
    relay_url          text NOT NULL,
    device_endpoint_id text,               -- filled by /link/lookup (the new device)
    mac                text,               -- HMAC(C, endpoint_id) — box verifies at approve
    status             text NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending', 'requested', 'approved', 'expired')),
    created_at         timestamptz NOT NULL DEFAULT now(),
    expires_at         timestamptz NOT NULL
);
CREATE INDEX IF NOT EXISTS link_session_expires_idx ON link_session (expires_at);
