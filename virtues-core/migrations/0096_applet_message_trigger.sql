-- 0096 — an applet can be spoken to.
--
-- Every wake until now was something the box did to itself: a clock, a poll, a
-- device pushing data, a tool call. A person could turn an applet on, off, or
-- run it — but could not tell it anything. That absence is why AGENTS.md
-- instructs the model to decline photo-logging, to decline personas, and to
-- downgrade the calorie tracker (the plan's own flagship Tracker) to "declare
-- a table and log it yourself with sql_write". Three refusals, one cause.
--
-- `message` is the sixth trigger: a run whose payload is something the user
-- typed. The plumbing already exists — `run_applet` takes a payload and the
-- webhook path uses it — so this is the enum catching up with the shape.
--
-- The run row is what makes it a conversation. `trigger = 'message'` plus the
-- text the person sent, answered by `result_summary`, IS the exchange; no
-- separate thread object is minted. That is deliberate. The plan reached for
-- correspondent threads and Appendix B deferred them because reply-as-input
-- and reply-as-edit collide in a single box — a chat thread makes "I had eggs"
-- and "make it weekly" the same kind of event and something downstream has to
-- guess which. A composer that sits on the detail page beside a visibly
-- separate prompt editor does not have that problem: the layout says which
-- verb you are using, so nothing has to guess.

ALTER TABLE app_applet_runs DROP CONSTRAINT IF EXISTS app_action_runs_trigger_check;
ALTER TABLE app_applet_runs DROP CONSTRAINT IF EXISTS app_applet_runs_trigger_check;

ALTER TABLE app_applet_runs
    ADD CONSTRAINT app_applet_runs_trigger_check
    CHECK (trigger IN ('cron', 'manual', 'tool', 'api', 'webhook', 'message'));

-- What the person actually said. Lives on the run rather than in a thread for
-- the reason above; nullable because every other trigger has nothing to say.
ALTER TABLE app_applet_runs ADD COLUMN message TEXT;
