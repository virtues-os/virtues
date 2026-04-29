-- Migration 049: data_health_active_energy + data_health_distance
--
-- iOS HealthKit sends active_energy and distance metrics that we previously
-- dropped on the floor (counted as "skipped" in the action summary). These
-- two new ontology tables capture them.
--
-- - active_energy: kcal burned, derived by the device from HR + motion. Useful
--   for "how active was your day" + workout correlation. Aggregates well into
--   hourly buckets for the autonomic curve.
--
-- - distance: meters travelled, derived by the device from accelerometer +
--   GPS. More accurate than steps for cycling/vehicle. Combines with location
--   visits for "moved A→B (1.2 km)" annotations.
--
-- Standard ontology shape: id PK, value, timestamp, source_stream_id UNIQUE,
-- provenance + metadata + audit timestamps. Same as data_health_steps.

CREATE TABLE IF NOT EXISTS data_health_active_energy (
    id TEXT PRIMARY KEY,
    kcal REAL NOT NULL,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_data_health_active_energy_ts
    ON data_health_active_energy(timestamp DESC);

CREATE TRIGGER IF NOT EXISTS data_health_active_energy_set_updated_at
    AFTER UPDATE ON data_health_active_energy
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_active_energy SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TABLE IF NOT EXISTS data_health_distance (
    id TEXT PRIMARY KEY,
    meters REAL NOT NULL,
    timestamp TEXT NOT NULL,
    source_stream_id TEXT NOT NULL UNIQUE,
    source_table TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_data_health_distance_ts
    ON data_health_distance(timestamp DESC);

CREATE TRIGGER IF NOT EXISTS data_health_distance_set_updated_at
    AFTER UPDATE ON data_health_distance
    FOR EACH ROW
    WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE data_health_distance SET updated_at = datetime('now') WHERE id = NEW.id;
END;
