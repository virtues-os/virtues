-- Anchor device-ingest actions on the device identity, not just its credential.
--
-- Device ingest (ios/mac, webhook trigger) is fanned out per credential today,
-- and `app_actions.credential_id` -> the device's bearer credential is the only
-- thing tying the action to the device. As inbound auth collapses to the proven
-- iroh key, the device bearer becomes vestigial — so record the device directly
-- on the action. This is the additive first step (populate + start using
-- device_id); the fan-out re-key + bearer removal follow once verified.
--
-- OAuth/API-key actions keep device_id = NULL (they're not device-owned).
ALTER TABLE app_actions
    ADD COLUMN device_id TEXT REFERENCES app_device(id) ON DELETE CASCADE;

CREATE INDEX idx_app_actions_device_id
    ON app_actions(device_id) WHERE device_id IS NOT NULL;

-- Backfill existing device-ingest actions from their credential's device_id.
UPDATE app_actions a
SET device_id = c.device_id
FROM credentials c
WHERE a.credential_id = c.id
  AND c.device_id IS NOT NULL;
