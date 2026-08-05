-- 0090 — the scheduler remembers which slot it owes.
--
-- Until now nothing anywhere recorded when an applet was *supposed* to run.
-- The scheduler registered a cron job, the job fired or it did not, and the
-- only trace either way was a run row that existed or a run row that did not.
-- Two things fall out of that absence:
--
--   · **Catch-up is impossible.** A box asleep at 7am misses the examen, and
--     on waking has no way to know it missed anything — there is no record of
--     an owed slot, only of runs that happened.
--
--   · **"Expected but didn't run" is uncomputable.** The needs-attention strip
--     can only surface applets whose last run *errored*. An applet that
--     silently stopped firing — unschedulable cron, a job that failed to
--     register, a box that was off — looks identical to one that is quietly
--     healthy. Silent non-execution is the failure mode a personal automation
--     system can least afford, and it was the one we could not see.
--
-- Two columns close both:
--
--   last_slot_at  the scheduled slot most recently *handled* — either fired,
--                 or consciously declined for catch-up. Not `started_at`: a
--                 catch-up run at 07:12 handles the 06:00 slot, and conflating
--                 the two makes the next slot compute from the wrong instant.
--
--   next_due_at   the first occurrence strictly after last_slot_at — the slot
--                 currently owed. Overdue is then simply `next_due_at` far
--                 enough in the past, and "next run" has an honest answer to
--                 show on the detail page.
--
-- Both are nullable and both are derived: the scheduler recomputes them from
-- `cron_schedule` on every sync pass, so a row that predates this migration,
-- or one whose schedule was just edited, heals on the next tick without a
-- backfill. That is also why there is no `catch_up` column — the plan settles
-- it as a doctrine default read from the schedule's shape (daily-or-less
-- catches up, hourly-or-more does not), and a column would only let the two
-- disagree.

ALTER TABLE app_applets ADD COLUMN last_slot_at TIMESTAMPTZ;
ALTER TABLE app_applets ADD COLUMN next_due_at  TIMESTAMPTZ;

-- The overdue sweep asks one question of the whole table on a timer: which
-- enabled cron applets are past due? Partial, because the rows it never wants
-- are the majority — everything without a schedule.
CREATE INDEX IF NOT EXISTS idx_app_applets_next_due
    ON app_applets (next_due_at)
    WHERE enabled = TRUE AND next_due_at IS NOT NULL;
