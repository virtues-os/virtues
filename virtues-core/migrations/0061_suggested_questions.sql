-- 0061_suggested_questions
--
-- The suggested question an empty new chat shows in its composer
-- (crate::api::suggestions). An hourly applet reads what changed since its
-- last run, asks one model call for a few questions the owner would
-- plausibly ask now, and writes them here. Opening a chat reads the best
-- unexpired one, so the screen never waits on a model.
--
-- app_suggestion_runs is one row per run that called the model, with the
-- fingerprint of the record it read, so the next run can skip a record that
-- has not changed. app_suggestions is one row per candidate, kept after it
-- expires as the ledger of what was offered and what the owner did with it.

CREATE TABLE app_suggestion_runs (
    id text PRIMARY KEY,
    -- What the run read: the latest write across the sources it draws on.
    fingerprint text NOT NULL,
    candidates integer NOT NULL DEFAULT 0 CHECK (candidates >= 0),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX app_suggestion_runs_created_at_idx ON app_suggestion_runs (created_at DESC);

CREATE TABLE app_suggestions (
    id text PRIMARY KEY,
    run_id text NOT NULL REFERENCES app_suggestion_runs (id) ON DELETE CASCADE,
    -- Where the question sits relative to now: something just ended
    -- (aftermath), is coming up (preparation), just arrived (arrival), was
    -- left unsettled (open_loop), or was unlike the owner's usual (anomaly).
    kind text NOT NULL
        CHECK (kind IN ('aftermath', 'preparation', 'arrival', 'open_loop', 'anomaly')),
    question text NOT NULL,
    -- The model's one line on why now. Kept for tuning, never shown.
    reason text,
    -- The records the question is about, as [{url, label}]. Accepting the
    -- question attaches them as mentions.
    refs jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- Order within the run, 1 first.
    rank integer NOT NULL CHECK (rank >= 1),
    expires_at timestamptz NOT NULL,
    -- Why the run kept it out of the composer, or null when it was offered.
    dropped_reason text,
    shown_at timestamptz,
    outcome text CHECK (outcome IN ('used', 'dismissed')),
    outcome_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX app_suggestions_offered_idx
    ON app_suggestions (expires_at DESC)
    WHERE dropped_reason IS NULL AND outcome IS NULL;
CREATE INDEX app_suggestions_created_at_idx ON app_suggestions (created_at DESC);
