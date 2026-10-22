-- 0060_request_spend_limits
--
-- The owner's spending limits for requests the assistant works in the
-- background (crate::task_runner, agents/plan/tasks-plan.md). One ceiling
-- for a single run and one across all request runs in a rolling day, in
-- micros-USD of wallet spend. Zero per day means requests never start.

ALTER TABLE app_assistant_profile
    ADD COLUMN request_run_limit_micros bigint NOT NULL DEFAULT 500000
        CHECK (request_run_limit_micros >= 0),
    ADD COLUMN request_day_limit_micros bigint NOT NULL DEFAULT 2000000
        CHECK (request_day_limit_micros >= 0);
