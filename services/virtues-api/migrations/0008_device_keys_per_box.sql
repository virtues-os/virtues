-- One key per BOX, not one key per account.
--
-- `register_device` has always ended with "delete every key for this account,
-- install this one" — correct for its original job (a box re-linking rotates
-- its own credential, and the wallet is untouched) and silently destructive for
-- the job it acquired (a second box joining an account kills the first).
--
-- Nobody hit it because linking a second box was laborious. It becomes a single
-- tap the moment the app can vouch for a link, so the destruction has to go
-- first.
--
-- The discriminator is the box's iroh EndpointId — self-certifying, already
-- known to atlas (`iroh_endpoints`), and already the box's identity everywhere
-- else. NULL means "a key from before this migration": rotation for those keeps
-- the old whole-account behaviour, so nothing in flight breaks.
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS box_id text;

-- A box has at most one live key; an account has as many boxes as it likes.
CREATE UNIQUE INDEX IF NOT EXISTS device_keys_account_box_idx
    ON device_keys (account_id, box_id)
    WHERE box_id IS NOT NULL;
