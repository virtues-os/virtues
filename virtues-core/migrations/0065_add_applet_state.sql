-- 0065_add_applet_state
--
-- What an applet's code hands back between runs (a sync's cursor, the last
-- id it saw) gets its own column. It lived in `config`, which reconcile
-- replaces with the manifest's on every boot for shipped applets, so every
-- reconcile wiped every sync's cursor. The runner now reads and writes
-- `state`, and reconcile never touches it.
--
-- Code applets' current config is copied over so a cursor that survived to
-- now is kept. Manifest keys copied along with it are harmless: the runner
-- lays `config` over `state`, so the manifest's value wins.

ALTER TABLE app_applets ADD COLUMN state jsonb NOT NULL DEFAULT '{}'::jsonb;

UPDATE app_applets SET state = config WHERE command IS NOT NULL;
