-- 0054: the applet-writer role — the scoped write path for applet tables.
--
-- The `sql_write` tool executes as this role, so applets (and the chat
-- agent logging on a user's behalf) can INSERT/UPDATE/DELETE **only inside
-- applet_* schemas** — never data_*, wiki_*, app_*, or anything else.
-- Enforcement is PG grants, not SQL parsing. Like the face reader (0052),
-- the role starts empty; per-schema grants are applied idempotently at
-- boot and after each setup_applet schema apply.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'virtues_applet_writer') THEN
        CREATE ROLE virtues_applet_writer NOLOGIN;
    END IF;
END $$;

GRANT virtues_applet_writer TO current_user;
