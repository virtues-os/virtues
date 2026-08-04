-- 0084 — Let an older binary survive the app_annotations rename.
--
-- 0082 renamed `app_annotations` to `app_marginalia`. The rename itself is
-- correct and cheap, but the DROP RULE it was written under does not cover it,
-- and that gap is real rather than theoretical: it showed up as
--
--     ERROR virtues::applet_runner: subprocess phase failed
--     applet_id="applet_embedding_index"
--     error: relation "app_annotations" does not exist
--
-- every fifteen minutes, from a binary compiled before the rename.
--
-- The expand/migrate/contract rule was written for DROPS, where old code keeps
-- working because the column is still there and the drop can wait a release.
-- A RENAME has no such window: the old name disappears the instant the
-- migration lands. So an older binary breaks immediately — and this box has two
-- paths that produce exactly that pairing automatically:
--
--   * `flip_back` (cli/upgrade.rs) — migrations succeed, service_start fails
--     for any unrelated reason, and it reverts to the PRIOR slot and starts it.
--     Old binary, new schema, no operator involved.
--   * `virtues rollback` — flips the binary only; the schema stays forward.
--
-- Boot migrations also run with `set_ignore_missing(true)`, so such a binary
-- boots cleanly and then fails per query, which is the worst failure shape
-- there is: healthy at startup, broken in use.
--
-- A view restores the old name. Postgres auto-updates a view over a single
-- table with no aggregates or DISTINCT, so SELECT/INSERT/UPDATE/DELETE all work
-- through it — the old binary cannot tell the difference.
--
-- Why a separate migration rather than an edit to 0082: `sqlx::migrate` keys on
-- a checksum of the file's bytes. Editing an applied migration changes that
-- checksum and the next upgrade refuses to run — the same reason 0052 was left
-- alone and its fix put in the CI harness instead. A migration is immutable
-- once it has run anywhere, including on a dev box.
--
-- DROP THIS a release after the one that renames. Its whole job is to cover the
-- window in which a rollback can land on a binary that predates 0082; once no
-- such binary is reachable, the view is a second name for one table and a
-- second name is how the next person gets confused.

CREATE VIEW app_annotations AS SELECT * FROM app_marginalia;

COMMENT ON VIEW app_annotations IS
    'Compatibility shim for binaries older than migration 0082, which renamed this table to app_marginalia. Drop one release after 0082 ships.';
