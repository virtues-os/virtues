-- 0033_add_device_push_address
--
-- The box has never had an ADDRESS for anything. `app_device.endpoint_id` is a
-- key — how a device proves it is itself — and every path into the box is
-- something else dialing in. A reminder is the first thing that needs the box
-- to go the other way, so a device row gains the other half of the pair.
--
-- See agents/plan/reminders-plan.md.
ALTER TABLE app_device ADD COLUMN push_address text;

-- NOT decoration, and not "created_at by another name". APNs answers a dead
-- token with `410 Unregistered` AND the timestamp at which it died. A send
-- that raced a fresh registration returns a 410 whose timestamp is OLDER than
-- our registration, and acting on that deletes a token that had just arrived —
-- the device then goes silently unreachable until the app is next opened.
-- Comparing against this column is the whole guard, so the two columns are
-- written together or not at all.
ALTER TABLE app_device ADD COLUMN push_address_at timestamptz;

ALTER TABLE app_device
  ADD CONSTRAINT app_device_push_address_pair
  CHECK ((push_address IS NULL) = (push_address_at IS NULL));

-- Every send reads this, and a box with one paired phone among several devices
-- should not scan for it.
CREATE INDEX app_device_push_address_idx ON app_device (push_address)
  WHERE push_address IS NOT NULL AND revoked_at IS NULL;
