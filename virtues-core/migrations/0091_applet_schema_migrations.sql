-- 0091 — an applet's tables get migrations, not one rewritten schema file.
--
-- `schema_sql` has been applied as "idempotent DDL by doctrine", which in
-- practice means CREATE TABLE IF NOT EXISTS. That works exactly once. Because
-- re-calling setup_applet IS the edit path, the second call rewrites
-- schema.sql and re-applies it — and IF NOT EXISTS, on a table that exists,
-- does nothing at all.
--
-- The failure is silent and it points the wrong way. A model adding a column
-- to a tracker gets a successful apply, believes the column is there, and
-- writes a prompt that uses it. Every later sql_write naming that column
-- fails at runtime, nightly, forever — the soft-failure mode AGENTS.md warns
-- about everywhere else. Nothing in the check could see it, because the DDL
-- was valid and the apply genuinely succeeded.
--
-- So each call's `schema_sql` becomes one numbered, append-only migration,
-- recorded here once applied and never re-run. Versions live beside the
-- manifest at applets/<slug>/schema/NNNN_*.sql, so the folder stays the whole
-- portable definition and a fresh box replays them in order to reach the same
-- shape.
--
-- `checksum` is what makes the common case free: re-setup that did not touch
-- the schema resubmits identical DDL, matches the recorded checksum, and is
-- recognized as already applied instead of appending a redundant version. It
-- is also the tripwire for a version edited after the fact — a file whose
-- contents no longer match what this box ran is a divergence worth refusing
-- rather than papering over.

CREATE TABLE app_applet_schema_migrations (
    applet_id   TEXT        NOT NULL REFERENCES app_applets(id) ON DELETE CASCADE,
    -- Ordinal within this applet, matching the NNNN prefix on disk.
    version     INTEGER     NOT NULL,
    -- Filename, so a human reading the table can find the SQL that ran.
    name        TEXT        NOT NULL,
    -- SHA-256 of the DDL text as applied.
    checksum    TEXT        NOT NULL,
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (applet_id, version)
);

-- ON DELETE CASCADE above is deliberate and differs from how run history is
-- treated. Runs are an audit trail worth keeping past their applet; this is
-- bookkeeping ABOUT tables that the delete path drops alongside the applet
-- (or deliberately keeps, in which case the applet is gone and there is
-- nothing left to migrate). Either way the rows have no meaning once the
-- applet is gone.
