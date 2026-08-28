-- The last open call of the 2026-08-28 schema audit (R9): hrv_z was never
-- written by any scorer — autonomic_scoring computes from hr_z only — yet
-- rendered end-to-end as a permanent null. Adam's call 2026-08-28: drop.
-- If HRV-based scoring ever lands, the column returns WITH its writer.
ALTER TABLE wiki_events DROP COLUMN IF EXISTS hrv_z;
