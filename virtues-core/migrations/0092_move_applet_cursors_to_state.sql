-- 0092_move_applet_cursors_to_state
--
-- A code applet's cursor (a Gmail `last_sync_unix`, Calendar `sync_tokens`,
-- a Plaid cursor) belongs in `state`. Migration 0065 copied `config` into
-- `state` but left the cursor keys in `config` too. The runner lays `config`
-- over `state` when it builds a run's input, and reconcile keeps every
-- `config` key the manifest does not own, so that leftover copy won on every
-- run. Each sync restarted from the cursor it had when 0065 ran: it fetched
-- the same mail, events and transactions again every tick and threw the
-- duplicates away.
--
-- For code applets, every `config` key the manifest does not own moves into
-- `state`. A value already in `state` is newer, so it wins. The keys the
-- manifest owns are the ones reconcile rewrites (see `applet_templates`).

UPDATE app_applets
SET state  = (config - 'limits' - 'model_slot' - 'allowed_tools' - 'net_hosts') || state,
    config = config - ARRAY(
        SELECT k FROM jsonb_object_keys(config) AS k
        WHERE k NOT IN ('limits', 'model_slot', 'allowed_tools', 'net_hosts')
    )
WHERE command IS NOT NULL;
