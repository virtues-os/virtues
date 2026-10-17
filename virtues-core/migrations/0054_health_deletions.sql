-- 0054_health_deletions
--
-- A sample deleted in Apple Health is tombstoned on the box, never deleted:
-- ios_ingest stamps deleted_at_source and readers skip the row. Every other
-- data_health_* table already has the column; these two were created without
-- it.

ALTER TABLE public.data_health_active_energy ADD COLUMN deleted_at_source timestamptz;
ALTER TABLE public.data_health_distance ADD COLUMN deleted_at_source timestamptz;
