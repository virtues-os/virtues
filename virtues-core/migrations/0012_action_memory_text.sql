-- `app_actions.memory` is a free-form markdown scratchpad that actions append
-- to across runs (see scheduler::actions::update_memory and the `Action.memory:
-- Option<String>` field). It was declared JSONB in 0004, but no code ever wrote
-- or read it as structured JSON — every writer binds a plain string and every
-- reader decodes `Option<String>`. The JSONB type only "worked" because the
-- column was always NULL; the first non-null write failed with
-- `column "memory" is of type jsonb but expression is of type text`, and a
-- non-null read would fail symmetrically. Make the type match its sole use.
ALTER TABLE app_actions
    ALTER COLUMN memory TYPE TEXT USING memory #>> '{}';
