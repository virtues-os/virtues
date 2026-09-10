-- Per-day attempt bookkeeping for the nightly narration chain.
--
-- The catch-up queue in `day_summary_eod` selects un-narrated days by
-- `narrated_at IS NULL`. Without a memory of how often a day has already been
-- tried, a deterministic failure (a model cap too low for the day's dossier, a
-- provider refusing the request) is retried every hour until the day ages out
-- of the queue's window, and then never again. Both halves of that are wrong:
-- the hourly retries burn a best-model call each, and the silent age-out is
-- how a day the owner actually lived stays unwritten forever.
--
-- `narration_attempts` counts AUTOMATIC (cron) attempts; the queue backs off
-- exponentially between them and parks the day after a fixed number. An
-- explicit-date run (chat tool, CLI, manual trigger) ignores the counter. A
-- successful re-cut of the day (new sources, new fingerprint) resets it — a
-- re-cut day is a new situation and earns a fresh budget.
ALTER TABLE wiki_days
    ADD COLUMN narration_attempts integer NOT NULL DEFAULT 0,
    ADD COLUMN narration_attempted_at timestamp with time zone;
