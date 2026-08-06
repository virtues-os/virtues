-- 0089 — `budget_exceeded` becomes a run status of its own.
--
-- An applet that hits its `config.limits.max_llm_cost` ceiling has not failed:
-- it was stopped, deliberately, by a limit its owner set. Recording that as
-- `error` files it in the needs-attention strip beside genuine breakage and
-- tells the reader to go fix something that is working exactly as configured.
-- It is also the only way "stopped on budget" can be counted apart from
-- "broke" — which is the number that tells you a cap is set too low.
--
-- The CHECK still carries its pre-rename name: 0053 renamed the tables and
-- 0076 the foreign key, but an inline CHECK keeps whatever Postgres generated
-- for it back at CREATE TABLE in 0004. Both spellings are dropped so this
-- applies cleanly on a box from either side of that history.

ALTER TABLE app_applet_runs DROP CONSTRAINT IF EXISTS app_action_runs_status_check;
ALTER TABLE app_applet_runs DROP CONSTRAINT IF EXISTS app_applet_runs_status_check;

ALTER TABLE app_applet_runs
    ADD CONSTRAINT app_applet_runs_status_check
    CHECK (status IN (
        'running',
        'success',
        'error',
        'cancelled',
        'skipped',
        'budget_exceeded'
    ));

-- Enforcement reads `app_ai_calls` two ways: by run (the per-run ceiling) and
-- by applet over a window (the per-day ceiling). Both go through
-- `applet_run_id`, a column nothing has ever written until now — so it has
-- never had an index, and the per-run sum would otherwise scan the whole
-- cost log on every agent step.
CREATE INDEX IF NOT EXISTS idx_app_ai_calls_applet_run
    ON app_ai_calls (applet_run_id, created_at)
    WHERE applet_run_id IS NOT NULL;
