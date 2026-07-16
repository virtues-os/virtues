-- Promote `speed` and `course` from `data_location_point.metadata` to first-class
-- columns. They are CLLocation kinematic properties — the same tier as `altitude`
-- and the `*_accuracy` fields, which are already columns — so they were simply
-- misfiled in the metadata jsonb. A real consumer now reads them (transit-mode
-- grounding for the day pipeline, and movement/WHERE novelty scoring), and a typed,
-- indexable column beats `(metadata->>'speed')::double precision` at every call site.
--
-- Backfill from the metadata the ingest has been writing all along, so historical
-- points gain the columns too. `->>` yields SQL NULL for a JSON null or a missing
-- key, so absent/invalid speeds stay NULL.

ALTER TABLE data_location_point
    ADD COLUMN speed  double precision,
    ADD COLUMN course double precision;

UPDATE data_location_point
SET speed  = (metadata ->> 'speed')::double precision,
    course = (metadata ->> 'course')::double precision
WHERE metadata ? 'speed'
   OR metadata ? 'course';
