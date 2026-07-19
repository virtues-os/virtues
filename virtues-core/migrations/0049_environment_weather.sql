-- Weather — the first "environment" ontology (the ambient world around you,
-- distinct from health = your body and calendar/communication = your acts).
-- Pulled hourly from Open-Meteo for the owner's location by the `weather_sync`
-- action. One table holds both actuals and forecast, the honest meteorological
-- way (issue-time vs valid-time): `is_forecast` + `valid_time`/`issued_at`.
-- Actuals are kept forever (tiny, and they let the box notice "the coldest
-- morning you've recorded"); forecast rows accumulate per issue and the reader
-- takes the freshest.

CREATE TABLE data_environment_weather (
    id                TEXT PRIMARY KEY,
    latitude          DOUBLE PRECISION NOT NULL,
    longitude         DOUBLE PRECISION NOT NULL,
    valid_time        TIMESTAMPTZ NOT NULL,          -- the time this weather is FOR
    issued_at         TIMESTAMPTZ NOT NULL,          -- when it was fetched / issued
    is_forecast       BOOLEAN NOT NULL DEFAULT FALSE,
    -- current-conditions fields (actuals)
    temperature_c     DOUBLE PRECISION,
    apparent_c        DOUBLE PRECISION,
    humidity_pct      DOUBLE PRECISION,
    precipitation_mm  DOUBLE PRECISION,
    wind_kph          DOUBLE PRECISION,
    is_day            BOOLEAN,
    -- daily-forecast fields
    temp_max_c        DOUBLE PRECISION,
    temp_min_c        DOUBLE PRECISION,
    sunrise           TIMESTAMPTZ,
    sunset            TIMESTAMPTZ,
    -- shared
    weather_code      INTEGER,                        -- WMO code
    -- provenance (matches the rest of the data_* layer)
    source_stream_id  TEXT NOT NULL UNIQUE,
    source_table      TEXT NOT NULL DEFAULT 'data_environment_weather',
    source_provider   TEXT NOT NULL DEFAULT 'open-meteo',
    metadata          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- freshest current actual: WHERE is_forecast = FALSE ORDER BY valid_time DESC
-- freshest forecast for a day: WHERE is_forecast AND valid_time::date = d ORDER BY issued_at DESC
CREATE INDEX idx_env_weather_valid ON data_environment_weather (is_forecast, valid_time DESC);
CREATE INDEX idx_env_weather_issued ON data_environment_weather (issued_at DESC);

COMMENT ON TABLE data_environment_weather IS
    'Weather (Open-Meteo) for the owner''s rounded location. Actuals and forecast in one table via is_forecast + valid_time/issued_at. The first "environment" ontology.';
