-- An applet's condition is SQL in its file's `run` now (agents/plan/
-- applet-shape.md, slice 7), and the runner no longer reads this column.
-- A row whose folder says otherwise gets run_sql from reconcile; this keeps
-- the gate of a row with no folder by moving it into run_sql, where the
-- runner reads it, so the applet still skips when it is false.
--
-- The column stays, emptied, so the previous binary still reads every row
-- after `virtues rollback`. A later migration drops it.
UPDATE app_applets
SET run_sql = 'select 1 where (' || condition || ')'
WHERE condition IS NOT NULL
  AND btrim(condition) <> ''
  AND run_sql IS NULL
  AND command IS NULL;

UPDATE app_applets SET condition = NULL WHERE condition IS NOT NULL;
