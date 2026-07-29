-- 0076 — Finish the actions→applets rename in the schema.
--
-- Migration 0053 renamed the tables (`app_actions` → `app_applets`,
-- `app_action_runs` → `app_applet_runs`) but left the columns and the
-- constraint/index names behind, because Postgres does not rename those when
-- you rename a table. So the schema has read `app_applet_runs.action_id` ever
-- since — the right table, the wrong word.
--
-- Renamed here:
--   app_applet_runs.action_id   → applet_id
--   app_ai_calls.action_run_id  → applet_run_id
--   plus the constraint and index names 0053 stranded.
--
-- NOT renamed, and not by oversight:
--
--   · `app_link_session.action_ids` — this one is on the wire. The device
--     pairing response serves it as the `action_ids` JSON key and the macOS
--     collector reads `json["action_ids"]` (Uploader.swift:309) to find its
--     webhook target. Renaming the column renames the key and breaks every
--     already-installed collector until it ships again, so it moves with the
--     HTTP surface, not with the schema.
--
--   · `wiki_events.agent_action`, `app_chats.action_instruction`,
--     `app_sudo_request.action` / `.action_payload` — four columns that share
--     a prefix and nothing else. An agent's action in an event record, a
--     chat's instruction, and a sudo request's payload are not applets and
--     never were.
--
-- The `action_` prefix on the id VALUES (`action_user__<slug>`) is likewise
-- untouched: those are data, they are parsed by `applet_schema_name()`, and
-- one of the two writers disagrees with the parser about how many underscores
-- to use. That gets its own migration once the bug is fixed.

ALTER TABLE app_applet_runs RENAME COLUMN action_id TO applet_id;
ALTER TABLE app_ai_calls    RENAME COLUMN action_run_id TO applet_run_id;

-- Names 0053 left pointing at the old word. Cosmetic, but a schema that
-- half-remembers its own history is how the next person gets confused.
ALTER INDEX  app_actions_pkey     RENAME TO app_applets_pkey;
ALTER INDEX  app_action_runs_pkey RENAME TO app_applet_runs_pkey;
ALTER TABLE  app_applet_runs
    RENAME CONSTRAINT app_action_runs_action_id_fkey TO app_applet_runs_applet_id_fkey;
