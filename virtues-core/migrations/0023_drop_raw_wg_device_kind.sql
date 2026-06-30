-- Drop the dead `raw_wg` device kind.
--
-- `raw_wg` was added in 0013 for box-generated WireGuard peers. The relay model
-- has no WireGuard, and after the box/client relay migration no code path ever
-- writes a device with this kind. Tighten the CHECK constraint back to the kinds
-- the box actually produces.
--
-- NOT safe to re-add the constraint blindly: the short-lived `virtues pair raw`
-- build (since reverted) DID create `raw_wg` rows, so a box that ran it would
-- still carry them and the tightened CHECK would fail mid-migration, bricking
-- the upgrade. Coerce any survivors to a permitted, inert kind first — these
-- peers are dead under the relay model regardless.
UPDATE app_device SET kind = 'cli' WHERE kind = 'raw_wg';

ALTER TABLE app_device DROP CONSTRAINT app_device_kind_check;
ALTER TABLE app_device ADD CONSTRAINT app_device_kind_check
    CHECK (kind IN ('browser', 'mobile_app', 'desktop_app', 'sensor', 'cli'));
