-- ---------------------------------------------------------------------------
-- Notebook items gain a `manuscript` role.
--
-- `role` has meant "does this member ground chat" since 0056 backfilled every
-- row to 'library'. That conflates two different things a notebook holds: the
-- material you are writing FROM, and the draft you are writing. Retrieval
-- currently treats them identically, so asking a notebook a question can
-- return your own unfinished chapter as a cited source.
--
-- 'manuscript' members stay in the notebook and stay visible, but are excluded
-- from retrieval scope (see resolve_notebook_scope). Nothing is migrated: every
-- existing member keeps role='library', and the UI is the only way to promote
-- one. 'pin' remains the nav-only role.
-- ---------------------------------------------------------------------------

ALTER TABLE app_notebook_items DROP CONSTRAINT IF EXISTS app_notebook_items_role_check;

ALTER TABLE app_notebook_items
    ADD CONSTRAINT app_notebook_items_role_check
    CHECK (role IN ('library', 'pin', 'manuscript'));
