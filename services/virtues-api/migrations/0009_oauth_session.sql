-- Server-side state for the three authorize-code OAuth providers (google,
-- notion, strava). Plaid already had this shape (0007); the others carried
-- their state INSIDE the provider's `state` parameter as base64(json) — which
-- is encoding, not signing, so a crafted `/start` link could name any
-- allowlisted return_url and the callback would deliver the exchange_token to
-- it. RFC 6749 §10.12/§10.15 and RFC 9700: `state` must be an opaque nonce
-- indexing server-side session, never a payload.
--
-- One row per authorize round-trip. `return_url` and `rust_state` are read
-- back from here at the callback, so the browser never gets to say where the
-- token goes. `code_verifier` is the PKCE secret (RFC 7636; S256 challenge
-- sent at authorize, verifier at token exchange) for providers that support
-- it. `exchange_sig` is the HMAC half of the minted exchange_token, set at
-- the callback and cleared by `consumed_at` on the box's single `/exchange`
-- call — a replayed token finds no unconsumed row.
--
-- Expired rows are swept opportunistically by `put_oauth_session`, like
-- plaid_link_session; no cron owns this table.
CREATE TABLE oauth_session (
    session_id    TEXT PRIMARY KEY,
    provider      TEXT        NOT NULL,
    return_url    TEXT        NOT NULL,
    rust_state    TEXT        NOT NULL,
    code_verifier TEXT,
    exchange_sig  TEXT UNIQUE,
    consumed_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL
);

CREATE INDEX oauth_session_expires_at_idx ON oauth_session (expires_at);
