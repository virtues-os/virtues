-- Add the `raw_wg` device kind: a box-generated WireGuard peer for a plain
-- (non-Virtues) client — e.g. a laptop that wants ssh over the tunnel, a
-- router, an esp32. Minted by `virtues pair raw`, which prints a ready
-- `wg-quick` config. It is a network peer, not a collector (no action
-- fan-out), but gets a durable, revocable credential row like any device.
--
-- The original CHECK is an inline column constraint, so Postgres auto-named it
-- `app_device_kind_check`. Drop and recreate it with the extra value.
ALTER TABLE app_device DROP CONSTRAINT app_device_kind_check;
ALTER TABLE app_device ADD CONSTRAINT app_device_kind_check
    CHECK (kind IN ('browser', 'mobile_app', 'desktop_app', 'sensor', 'cli', 'raw_wg'));
