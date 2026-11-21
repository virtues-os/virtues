-- Outlook sign-ins join Google's (routes/vendor.rs).
--
-- Both are connected through Truto's verified apps until Virtues has its own,
-- so one table holds every such connection and `provider` says whose it is.
-- A box calls `/v1/<provider>/c/<id>/...`, and the route looks the Truto id
-- up by id, provider AND the caller's account, so a Google connection cannot
-- be reached through the Outlook route or the other way round.
ALTER TABLE google_connections RENAME TO vendor_connections;
ALTER TABLE vendor_connections RENAME CONSTRAINT google_connections_pkey TO vendor_connections_pkey;
ALTER TABLE vendor_connections RENAME CONSTRAINT google_connections_truto_account_id_key TO vendor_connections_truto_account_id_key;
ALTER INDEX google_connections_account_idx RENAME TO vendor_connections_account_idx;

-- Every row before this migration is a Google connection.
ALTER TABLE vendor_connections ADD COLUMN provider TEXT NOT NULL DEFAULT 'google';
ALTER TABLE vendor_connections ALTER COLUMN provider DROP DEFAULT;
