-- 0086_add_note_history_and_mutes
--
-- Two tables for a note's page (`api::notes`).
--
-- `app_note_events`: what happened to a note, keyed by the note's key, for
-- its history. `raised` is written the first time your server works the
-- note out (once per key, by the partial unique index), `set_aside` when the
-- owner picks "Not now", and `brought_back` when they undo it.
--
-- `app_note_mutes`: kinds of note the owner asked for fewer of ("Fewer like
-- this"). `rule` is the note's kind and `scope` narrows it: for a clash, the
-- two thread ids in sorted order joined by a colon; for a task with no time
-- set aside, the thread id. A muted note is not listed anywhere, and its
-- page still opens so the owner can turn the mute off.
--
-- Rolling back to an older release: older releases ignore both tables.

CREATE TABLE app_note_events (
    id          bigserial PRIMARY KEY,
    key         text NOT NULL,
    kind        text NOT NULL CHECK (kind IN ('raised', 'set_aside', 'brought_back')),
    occurred_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_app_note_events_key ON app_note_events (key, occurred_at);
CREATE UNIQUE INDEX idx_app_note_events_raised ON app_note_events (key) WHERE kind = 'raised';

CREATE TABLE app_note_mutes (
    rule       text NOT NULL CHECK (rule IN ('clash', 'unplanned')),
    scope      text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (rule, scope)
);
