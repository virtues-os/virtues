-- A connect that a box registered before sending the owner's browser.
--
-- `/start` is a browser navigation, so on its own it cannot say which box
-- started it, and `/exchange` served whoever held the exchange token. Anyone
-- could then send an owner a `/start` link naming an address they control,
-- and receive the owner's provider tokens once the owner approved.
--
-- Now the box calls `POST /{provider}/sessions` with its key first. That
-- writes a row here and returns the ticket, and the browser goes to
-- `/start?ticket=…`. `/start` deletes the row as it reads it and copies
-- `key_hash` onto the session it opens (`oauth_session`, or
-- `plaid_link_session` for Plaid). `/exchange` then serves only the key
-- with that hash.
--
-- `key_hash` is sha256 of the box's api_key, as in `device_keys`. It is NULL on
-- sessions started without a ticket, by boxes that predate `/sessions`;
-- `OAUTH_REQUIRE_REGISTERED` refuses those once the fleet has moved.
--
-- Rows live for five minutes and `put_oauth_ticket` sweeps expired ones, so
-- no cron owns this table. An older image ignores the table and the new
-- columns, so a rollback keeps working.
CREATE TABLE oauth_ticket (
    ticket      TEXT PRIMARY KEY,
    provider    TEXT        NOT NULL,
    return_url  TEXT        NOT NULL,
    rust_state  TEXT        NOT NULL,
    key_hash    BYTEA       NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX oauth_ticket_expires_at_idx ON oauth_ticket (expires_at);

ALTER TABLE oauth_session ADD COLUMN key_hash BYTEA;
ALTER TABLE plaid_link_session ADD COLUMN key_hash BYTEA;
