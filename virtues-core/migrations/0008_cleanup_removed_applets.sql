-- 0008_cleanup_removed_applets
--
-- Biscuit and Calorie Tracker were deleted from applets/ (2026-08-26), but
-- reconcile only GCs system-owned rows and both shipped with owner = 'user'
-- — so any box that ever reconciled them keeps orphaned, faceless rows in
-- its applets list forever. These two ids could only have come from our
-- templates (chat-authored applets mint under the applet_user__ prefix), so
-- deleting them by id touches nothing a person made.
--
-- FKs do the right thing on delete: app_applet_runs.applet_id is
-- ON DELETE SET NULL (run history survives, unattributed) and
-- app_applet_schema_migrations cascades. The applet_calorie_tracker SCHEMA
-- and its tables are deliberately NOT dropped — logged meals are the
-- owner's record, and a cleanup migration does not delete a record.
DELETE FROM app_applets
 WHERE id IN ('applet_hello_world', 'applet_calorie_tracker');
