-- 0053: the full break — actions become applets at the schema level.
--
-- Pre-customer rename (docs/applets-overhaul-plan.md): the durable names
-- freeze into deployments, so they change NOW, once. Old migrations stay
-- untouched history; a fresh install creates app_actions at 0004 and
-- renames here, identically to an upgrading box.

ALTER TABLE app_actions      RENAME TO app_applets;
ALTER TABLE app_action_runs  RENAME TO app_applet_runs;

-- Index/constraint hygiene so nothing carries the old noun.
ALTER INDEX IF EXISTS idx_app_actions_enabled        RENAME TO idx_app_applets_enabled;
ALTER INDEX IF EXISTS idx_app_actions_credential_id  RENAME TO idx_app_applets_credential_id;
ALTER INDEX IF EXISTS idx_app_actions_device_id      RENAME TO idx_app_applets_device_id;
ALTER TABLE app_applets RENAME CONSTRAINT app_actions_owner_check TO app_applets_owner_check;
ALTER INDEX IF EXISTS idx_app_action_runs_action RENAME TO idx_app_applet_runs_applet;
ALTER INDEX IF EXISTS idx_app_action_runs_status RENAME TO idx_app_applet_runs_status;
ALTER INDEX IF EXISTS idx_app_action_runs_parent RENAME TO idx_app_applet_runs_parent;
