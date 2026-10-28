-- Where a box's MCP connection sign-in comes back to (routes/connections.rs).
--
-- A box registers `/connections/callback` here as its redirect URI, because
-- the owner's browser cannot always reach the box: on a phone, the only
-- address for it is a listener inside the Virtues app, which Safari cannot
-- reach. The service sends the browser here with the code; this row keeps it
-- until the box claims it by `state`, server to server.
--
-- The code is useless here. Every connection signs in with PKCE as a public
-- client, so exchanging the code takes the verifier, and only the box has
-- that. Rows are deleted when claimed, and expired ones are swept by the
-- callback, which is this table's only inserter.
CREATE TABLE connection_return (
    state      TEXT PRIMARY KEY,
    code       TEXT,
    issuer     TEXT,
    error      TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX connection_return_expires_at_idx ON connection_return (expires_at);
