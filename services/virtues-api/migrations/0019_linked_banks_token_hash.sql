-- Which access token belongs to each linked bank, so a bank call can be
-- refused when the token is not one of the caller's own (routes/plaid.rs).
--
-- `token_hash` is sha256 of Plaid's access token. The token itself stays on
-- the owner's server; a hash is nothing Plaid's API accepts. NULL means the
-- bank was recorded before this column existed: its first call looks the bank
-- up at Plaid and fills the hash in (`banks::claim`).
ALTER TABLE linked_banks ADD COLUMN IF NOT EXISTS token_hash bytea;

CREATE INDEX IF NOT EXISTS linked_banks_token_hash
    ON linked_banks (token_hash) WHERE token_hash IS NOT NULL;
