-- Outlook sign-ins get their own table (routes/vendor.rs), and Google's stay
-- in `google_connections`.
--
-- 0017 put both providers in `google_connections` behind a `provider`
-- column. That broke a rollback: the image before Outlook sweeps
-- `google_connections` for an account's Google connections with no
-- `provider` filter, so listing Google would delete every Outlook row, and
-- rolling forward again would give each Outlook connection a new id that no
-- box knows. The older image never reads this table, so nothing it does can
-- reach an Outlook connection.
--
-- `google_connections.provider` stays, always 'google', because 0017 is
-- applied and an image of 0017's era filters on it.
CREATE TABLE outlook_connections (
    id               TEXT PRIMARY KEY,
    account_id       TEXT NOT NULL,
    truto_account_id TEXT NOT NULL UNIQUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX outlook_connections_account_idx ON outlook_connections (account_id);

-- 0017's image may have recorded Outlook connections in the Google table.
INSERT INTO outlook_connections (id, account_id, truto_account_id, created_at)
SELECT id, account_id, truto_account_id, created_at FROM google_connections WHERE provider = 'outlook'
ON CONFLICT DO NOTHING;
DELETE FROM google_connections WHERE provider = 'outlook';
