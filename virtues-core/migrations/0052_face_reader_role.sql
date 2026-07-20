-- 0052: the face-reader role — default-deny data door for applet faces.
--
-- Applet faces (sandboxed-iframe HTML) query through one endpoint that
-- executes as this role inside a READ ONLY transaction. The role starts
-- with nothing; SELECT grants on data_* / wiki_* tables and applet_*
-- schemas are applied idempotently at server boot (and after reconcile)
-- so newly created tables are covered without new migrations.

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'virtues_face_reader') THEN
        CREATE ROLE virtues_face_reader NOLOGIN;
    END IF;
END $$;

-- The pool's login role must hold the member role to SET ROLE into it.
GRANT virtues_face_reader TO current_user;
