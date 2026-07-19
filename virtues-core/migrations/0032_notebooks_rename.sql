-- ---------------------------------------------------------------------------
-- Spaces → Notebooks rename + consolidation (Phase 1)
--
-- Renames the half-built "Spaces" paradigm to first-class "Notebooks" and adds
-- the columns the notebook model needs (persistent instructions, archive state,
-- and a member role distinguishing retrievable Library materials from nav-only
-- pins). Additive + rename only — preserves existing rows (their `space_…` ids
-- stay valid opaque text; new notebooks mint `nb_…`).
--
-- Notebooks live in the core `virtues` DB only. This does NOT touch the
-- billing/entitlement DBs (atlas / virtues-api).
-- ---------------------------------------------------------------------------

ALTER TABLE app_spaces      RENAME TO app_notebooks;
ALTER TABLE app_space_items RENAME TO app_notebook_items;

ALTER TABLE app_notebook_items RENAME COLUMN space_id TO notebook_id;
ALTER TABLE app_chats          RENAME COLUMN space_id TO notebook_id;

ALTER INDEX idx_app_spaces_name  RENAME TO idx_app_notebooks_name;
ALTER INDEX idx_app_spaces_sort  RENAME TO idx_app_notebooks_sort;
ALTER INDEX idx_space_items_space RENAME TO idx_notebook_items_notebook;
ALTER INDEX idx_space_items_url   RENAME TO idx_notebook_items_url;
ALTER INDEX idx_chats_space       RENAME TO idx_chats_notebook;

-- New notebook columns (UI wiring lands in later phases; cheap to add now).
--   instructions : persistent system prompt, distinct from the transient
--                  current_status "state of the room" memo.
--   archived_at  : notebook lifecycle (active vs archived).
ALTER TABLE app_notebooks ADD COLUMN instructions TEXT;
ALTER TABLE app_notebooks ADD COLUMN archived_at  TIMESTAMPTZ;

-- Member role: `library` (retrievable material that grounds chat) vs `pin`
-- (nav-only shortcut). Defaults to `pin`; the library/pin semantics are wired
-- in a later phase.
ALTER TABLE app_notebook_items ADD COLUMN role TEXT NOT NULL DEFAULT 'pin'
    CHECK (role IN ('library', 'pin'));
