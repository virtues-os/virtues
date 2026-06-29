-- 0021 — Box-local system telemetry time-series.
--
-- The live system monitor (api/system_telemetry.rs) only ever showed "now" —
-- its 48-point sparkline lived in the browser and died on reload. This table
-- persists a periodic sample so the Telemetry tab can render real history
-- (CPU/mem/GPU/net/temp) across restarts. Box-local, no egress.
--
-- A background sampler inserts one row per minute. Retention is "keep
-- everything" for now (rows are tiny; ~0.5M/yr); prune manually via the SQL tab
-- if disk ever tightens.
CREATE TABLE app_system_samples (
    id                 BIGSERIAL PRIMARY KEY,
    sampled_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    cpu_pct            REAL,
    mem_used_bytes     BIGINT,
    mem_total_bytes    BIGINT,
    gpu_pct            REAL,
    gpu_offload_active BOOLEAN,
    net_rx_bps         BIGINT,
    net_tx_bps         BIGINT,
    disk_used_bytes    BIGINT,
    disk_total_bytes   BIGINT,
    temp_c             REAL,
    load1              REAL,
    sidecar_embed_up   BOOLEAN,
    sidecar_rerank_up  BOOLEAN
);

CREATE INDEX idx_app_system_samples_sampled_at ON app_system_samples (sampled_at);
