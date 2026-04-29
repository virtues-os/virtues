-- Migration 053: Collapse multi-space carousel into single workspace
--
-- The multi-space model (Personal/Work/Health contexts) is replaced by
-- Projects (curated reference bundles you @-mention in chat). This migration:
--
--   1. Repoints all views and space_items from user spaces to the system space.
--   2. Deletes non-system space rows.
--   3. Drops the now-meaningless active_tab_state_json column.
--
-- Tab/pane state is managed client-side in localStorage. No data is lost;
-- views and items are merged into the surviving space_system row.

-- Step 1: Move orphaned views to system space
UPDATE app_views SET space_id = 'space_system' WHERE space_id != 'space_system';

-- Step 2: Move root-level space items to system space (avoid unique constraint
-- violations by deleting duplicates first)
DELETE FROM app_space_items
WHERE space_id IS NOT NULL
  AND space_id != 'space_system'
  AND url IN (
    SELECT url FROM app_space_items WHERE space_id = 'space_system'
  );

UPDATE app_space_items SET space_id = 'space_system' WHERE space_id IS NOT NULL AND space_id != 'space_system';

-- Step 3: Delete non-system spaces
DELETE FROM app_spaces WHERE id != 'space_system';

-- Step 4: Mark the surviving space as system (safety — already true for space_system)
UPDATE app_spaces SET is_system = TRUE WHERE id = 'space_system';
