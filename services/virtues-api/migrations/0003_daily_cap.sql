-- Per-customer daily spend ceiling, carried across the wall on the voucher.
--
-- Atlas owns the user-tunable value (`customers.daily_cap_micros`); it knows
-- the customer, so it reads the cap and embeds it in each minted voucher. This
-- service stores it on the voucher row, copies it onto the entitlement at
-- redeem, and enforces it in `charge()` — never learning who the customer is.
-- The wall holds: the cap is a number, not an identity column.
--
-- Default matches Atlas's `customers.daily_cap_micros` default ($20/day) so
-- existing rows backfill safely, and a pre-wire voucher (no field on the wire)
-- deserializes to the same floor that `charge()` used as a hardcoded constant
-- before this migration. A cap change takes effect at the customer's next
-- voucher / top-up (the natural privacy-preserving latency).

ALTER TABLE vouchers     ADD COLUMN daily_cap_micros bigint NOT NULL DEFAULT 20000000;
ALTER TABLE entitlements ADD COLUMN daily_cap_micros bigint NOT NULL DEFAULT 20000000;
