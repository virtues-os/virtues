-- Google sign-ins made through a verified app we do not run (routes/google.rs).
--
-- Until Virtues has its own verified Google app, a Google account can be
-- connected through Truto's. Truto keeps the Google grant and knows each
-- connection by its own id. This table says which Virtues account each of
-- those belongs to, so a box can only ever reach its own: every proxied call
-- names `id`, and the route looks the Truto id up by `id` AND the caller's
-- account.
--
-- Nothing here names a person. `account_id` is atlas's opaque id, with no
-- foreign key (like `device_keys` since 0011), and the Google address is
-- never stored: it is read from Truto when a box lists its connections.
CREATE TABLE google_connections (
    id               TEXT PRIMARY KEY,
    account_id       TEXT NOT NULL,
    truto_account_id TEXT NOT NULL UNIQUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX google_connections_account_idx ON google_connections (account_id);
