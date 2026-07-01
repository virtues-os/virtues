-- iroh reach: each paired device carries its own Ed25519 iroh EndpointId,
-- submitted at pairing. The box's iroh transport allowlists the set of
-- non-revoked node_ids (a transport-level ACL beneath the app-layer bearer/cookie
-- authorization). Nullable for legacy/browser rows that predate iroh or don't
-- run an iroh endpoint.
ALTER TABLE app_device ADD COLUMN node_id text;

-- One ACTIVE device per EndpointId. Partial unique index: multiple NULLs are
-- allowed, and it's scoped to non-revoked rows so a device that re-pairs with a
-- stable iroh key (its old row now revoked) doesn't collide with itself.
CREATE UNIQUE INDEX app_device_node_id_key
    ON app_device (node_id)
    WHERE node_id IS NOT NULL AND revoked_at IS NULL;
