-- A box key belongs to an account; a wallet is what the account has paid for.
--
-- `accounts` here was only ever a wallet: a balance and the date it expires.
-- The account itself is atlas's (an opaque `acct_…` id and an email), and
-- atlas never deletes it. But `device_keys.account_id` pointed at the wallet
-- with ON DELETE CASCADE, so when the sweeper reclaimed a lapsed wallet it
-- deleted the box's key too. Every owner who never paid lost theirs 37 to 68
-- days after linking, and with it everything the key unlocks for free, such
-- as the OAuth token proxy that keeps Google sync running.
--
-- So the key stops depending on the wallet. `device_keys.account_id` is
-- atlas's account id, with no foreign key, and the table is renamed to what it
-- holds. The ledger is the wallet's history and still goes with it.
--
-- A key now ends only when its box links again (rotation), when the blocklist
-- stops it, or when it is deleted on purpose.

ALTER TABLE device_keys DROP CONSTRAINT IF EXISTS device_keys_account_id_fkey;

ALTER TABLE accounts RENAME TO wallets;
ALTER INDEX accounts_pkey RENAME TO wallets_pkey;
ALTER TRIGGER accounts_bump_updated_at ON wallets RENAME TO wallets_bump_updated_at;

-- The image this deploy replaces keeps serving while the new one is smoke
-- tested against this database, and it is the image a rollback starts. It
-- reads and writes `accounts`, so `accounts` stays as a view onto `wallets`
-- (a simple view, so Postgres updates through it, ON CONFLICT included).
-- Drop it once no running or rollback image is older than this migration.
CREATE VIEW accounts AS
    SELECT account_id, balance_micros, expires_at, created_at, updated_at
    FROM wallets;
