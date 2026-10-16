-- 0053_add_health_cycle
--
-- Menstrual cycle tracking, raw samples only. Three tables hold what Apple
-- Health stores, one row per HealthKit sample, deduped on the sample UUID like
-- every other data_health_* table. Cycles, predictions and estimates are
-- transforms over these rows and are not stored here: HealthKit has no cycle
-- object, and Apple's own estimates are not readable by any route. See
-- agents/plan/cycle-tracking-plan.md.
--
-- Every row carries the phone's calendar days (started_on / ended_on,
-- inclusive) beside the sample's instants. These are mostly all-day samples,
-- and converting a UTC midnight on the box would put an entry logged while
-- travelling on the wrong day.
--
-- These tables are hidden from the assistant until the owner turns access on.
-- That is enforced by ensure_applet_db_grants (server/faces.rs), which skips
-- them, not here: virtues_face_reader is created in code, after migrations.
--
-- The CHECKs are a backstop. ios_ingest validates every row before inserting,
-- because a CHECK failure rejects the whole batch and the phone would retry it
-- forever.

CREATE TABLE public.data_health_cycle_log (
    id                  text PRIMARY KEY,
    kind                text NOT NULL,
    value               text,
    temperature_celsius double precision,
    is_cycle_start      boolean,
    started_on          date NOT NULL,
    ended_on            date,
    started_at          timestamptz NOT NULL,
    ended_at            timestamptz,
    source_stream_id    text NOT NULL UNIQUE,
    source_table        text NOT NULL,
    source_provider     text NOT NULL,
    deleted_at_source   timestamptz,
    is_archived         boolean NOT NULL DEFAULT false,
    metadata            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT data_health_cycle_log_kind_check CHECK (kind IN (
        'flow', 'spotting', 'basal_temperature', 'ovulation_test',
        'cervical_mucus', 'contraceptive', 'pregnancy', 'lactation', 'deviation')),
    CONSTRAINT data_health_cycle_log_value_check CHECK (CASE kind
        WHEN 'flow'           THEN value IN ('unspecified', 'light', 'medium', 'heavy', 'none')
        WHEN 'ovulation_test' THEN value IN ('negative', 'lh_surge', 'estrogen_surge', 'indeterminate')
        WHEN 'cervical_mucus' THEN value IN ('dry', 'sticky', 'creamy', 'watery', 'egg_white')
        WHEN 'contraceptive'  THEN value IN ('unspecified', 'implant', 'injection', 'iud', 'ring', 'oral', 'patch')
        WHEN 'deviation'      THEN value IN ('irregular', 'infrequent', 'prolonged', 'persistent_spotting')
        ELSE value IS NULL END),
    CONSTRAINT data_health_cycle_log_temperature_check
        CHECK ((kind = 'basal_temperature') = (temperature_celsius IS NOT NULL)),
    -- Only a cycle factor (an interval) may still be running.
    CONSTRAINT data_health_cycle_log_open_check
        CHECK (ended_on IS NOT NULL OR kind IN ('contraceptive', 'pregnancy', 'lactation')),
    CONSTRAINT data_health_cycle_log_order_check
        CHECK (ended_on IS NULL OR ended_on >= started_on)
);

CREATE INDEX idx_health_cycle_log_kind_day ON public.data_health_cycle_log (kind, started_on);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON public.data_health_cycle_log
    FOR EACH ROW EXECUTE FUNCTION public.tg_set_updated_at();

-- Every HealthKit symptom type, cycle-related or not. `symptom` is not
-- constrained here: Apple adds types, and the ingest's list is the gate.
-- `value` follows the type's HealthKit enum: severity for most, presence for
-- mood_changes and sleep_changes, its own enum for appetite_changes.
CREATE TABLE public.data_health_symptom (
    id                  text PRIMARY KEY,
    symptom             text NOT NULL,
    value               text NOT NULL,
    started_on          date NOT NULL,
    ended_on            date NOT NULL,
    started_at          timestamptz NOT NULL,
    ended_at            timestamptz NOT NULL,
    source_stream_id    text NOT NULL UNIQUE,
    source_table        text NOT NULL,
    source_provider     text NOT NULL,
    deleted_at_source   timestamptz,
    is_archived         boolean NOT NULL DEFAULT false,
    metadata            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT data_health_symptom_value_check CHECK (CASE
        WHEN symptom IN ('mood_changes', 'sleep_changes')
            THEN value IN ('present', 'not_present')
        WHEN symptom = 'appetite_changes'
            THEN value IN ('unspecified', 'no_change', 'decreased', 'increased')
        ELSE value IN ('unspecified', 'not_present', 'mild', 'moderate', 'severe') END),
    CONSTRAINT data_health_symptom_order_check CHECK (ended_on >= started_on)
);

CREATE INDEX idx_health_symptom_day ON public.data_health_symptom (symptom, started_on);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON public.data_health_symptom
    FOR EACH ROW EXECUTE FUNCTION public.tg_set_updated_at();

-- Apple Watch's nightly sleeping wrist temperature, absolute. Apple shows it
-- against a baseline it never exposes, so any baseline is computed from these
-- rows. `night_of` is the phone's calendar day when the night began.
CREATE TABLE public.data_health_wrist_temperature (
    id                  text PRIMARY KEY,
    temperature_celsius double precision NOT NULL,
    night_of            date NOT NULL,
    started_at          timestamptz NOT NULL,
    ended_at            timestamptz NOT NULL,
    source_stream_id    text NOT NULL UNIQUE,
    source_table        text NOT NULL,
    source_provider     text NOT NULL,
    deleted_at_source   timestamptz,
    is_archived         boolean NOT NULL DEFAULT false,
    metadata            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_health_wrist_temperature_night ON public.data_health_wrist_temperature (night_of);
CREATE TRIGGER set_updated_at BEFORE UPDATE ON public.data_health_wrist_temperature
    FOR EACH ROW EXECUTE FUNCTION public.tg_set_updated_at();
