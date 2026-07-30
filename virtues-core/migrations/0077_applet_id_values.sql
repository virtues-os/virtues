-- 0077 — Layer 3: the `action_` prefix on applet id VALUES.
--
-- 0053 renamed the tables, 0076 renamed the columns, and the ids themselves
-- still read `action_credential_refresh`. These are data, not schema: every id
-- is minted from one of two literals —
--
--   shipped:  format!("action_{}", dir.replace('/', "__"))   applet_templates
--   user:     USER_APPLET_PREFIX = "action_user__"           scheduler::applets
--
-- — both of which move to `applet_` in the same commit as this migration. The
-- prefix is also PARSED (`applet_schema_name`, `dir_for_applet_id`), so code
-- and data have to cross together or user applets stop resolving their private
-- schema. Nothing here is reversible by halves.
--
-- The FK is `ON DELETE SET NULL` with no update rule, so the parent id cannot
-- be rewritten while it is referenced. Dropped, both sides rewritten, then
-- restored — with `ON UPDATE CASCADE` added, so the next id change is one
-- statement instead of this dance.

ALTER TABLE app_applet_runs DROP CONSTRAINT app_applet_runs_applet_id_fkey;

-- `left(id, 7)` rather than LIKE: no escape ambiguity about the underscore,
-- and it cannot match an `action_` occurring later in the string.
UPDATE app_applets
   SET id = 'applet_' || substring(id from 8)
 WHERE left(id, 7) = 'action_';

UPDATE app_applet_runs
   SET applet_id = 'applet_' || substring(applet_id from 8)
 WHERE left(applet_id, 7) = 'action_';

ALTER TABLE app_applet_runs
    ADD CONSTRAINT app_applet_runs_applet_id_fkey
    FOREIGN KEY (applet_id) REFERENCES app_applets(id)
    ON DELETE SET NULL
    ON UPDATE CASCADE;

-- The pairing response's applet-id fanout. The COLUMN keeps its name — that is
-- the `action_ids` JSON key the macOS collector reads, and it moves with the
-- HTTP surface — but the VALUES inside it are applet ids and must track the
-- rename, or a freshly paired device gets handed ids that no longer exist.
UPDATE app_link_session
   SET action_ids = (
        SELECT jsonb_object_agg(
                 key,
                 CASE WHEN left(value #>> '{}', 7) = 'action_'
                      THEN to_jsonb('applet_' || substring(value #>> '{}' from 8))
                      ELSE value
                 END)
          FROM jsonb_each(action_ids)
       )
 WHERE action_ids <> '{}'::jsonb;

-- Collectors already installed hold the OLD id in their on-disk pair config
-- and will 404 against it. That is handled client-side rather than with an
-- alias table here: the mac collector now treats a webhook 404 as "this id is
-- gone", clears it, and refetches from `/api/devices/action-ids` on the next
-- cycle (fix/collector-stale-applet-id). A collector predating that fix will
-- retry the dead id forever and needs re-pairing — so ship that fix first.
