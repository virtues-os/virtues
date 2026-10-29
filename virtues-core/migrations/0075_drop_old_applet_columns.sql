-- 0075_drop_old_applet_columns
--
-- Four app_applets columns that nothing reads any more
-- (agents/plan/applet-shape.md):
--
-- - condition: an applet's gate is SQL in its file's `run`, where 0072 moved
--   every condition a row still had.
-- - agent_shipped: a change to a shipped applet is an override file, and
--   reconcile rewrites the row from the file. A prompt the owner edited in a
--   row is not carried into an override.
-- - net_hosts and connections: an applet has no per-applet approval of the
--   hosts its code reaches or the connections it uses.
--
-- A release from before this one reads `condition` on every applet and
-- writes it and `agent_shipped` on every reconcile. After `virtues rollback`
-- to one of those releases, its applets fail to load until the box upgrades
-- again.

ALTER TABLE app_applets DROP COLUMN condition;
ALTER TABLE app_applets DROP COLUMN agent_shipped;
ALTER TABLE app_applets DROP COLUMN net_hosts;
ALTER TABLE app_applets DROP COLUMN connections;
