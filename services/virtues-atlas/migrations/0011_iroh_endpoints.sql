-- iroh reach layer: map each node's Ed25519 EndpointId (the box + its paired
-- devices) to the owning account, so the relay's HTTP auth-callout
-- (`/relay/authorize`) can gate connections to active-subscription accounts.
--
-- Identity only — NO traffic, volume, or timing is stored. The relay stays blind;
-- this table lets atlas answer a yes/no "is this EndpointId a paying account?".
CREATE TABLE iroh_endpoints (
    endpoint_id text PRIMARY KEY,                 -- hex-encoded Ed25519 EndpointId
    account_id  text NOT NULL,                    -- opaque account (customers.account_id)
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX iroh_endpoints_account_idx ON iroh_endpoints (account_id);
