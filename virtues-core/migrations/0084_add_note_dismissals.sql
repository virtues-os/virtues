-- 0084_add_note_dismissals
--
-- Notes (`api::notes`) are worked out each time they are read, from the
-- owner's tasks, threads and calendar, so a note leaves on its own when its
-- condition clears. "Not now" sets one aside: this table remembers which.
-- A note's key names the condition (the events or the task and its day), so
-- a changed condition is a new note and shows again.
--
-- Rolling back to an older release: that release has no notes, so the table
-- is ignored.

CREATE TABLE app_note_dismissals (
    key text PRIMARY KEY,
    dismissed_at timestamptz NOT NULL DEFAULT now()
);
