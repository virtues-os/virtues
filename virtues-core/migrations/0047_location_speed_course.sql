-- Promote `speed` and `course` from `data_location_point.metadata` to first-class
-- columns. They are CLLocation kinematic properties — the same tier as `altitude`
-- and the `*_accuracy` fields, which are already columns — so they were simply
-- misfiled in the metadata jsonb, and a typed column beats
-- `(metadata->>'speed')::double precision` at every call site.
--
-- ADD COLUMN only — deliberately NO backfill UPDATE. `data_location_point` is the
-- highest-volume table (continuous GPS), and a full-table rewrite at migration time
-- would take a minutes-long exclusive lock on a mature box during `virtues upgrade`.
-- New rows write the columns; historical `speed` remains in `metadata->>'speed'`, and
-- the only reader (day movement segmentation) derives pace from the raw lat/lon trace
-- when the column is absent — so no backfill is needed.

ALTER TABLE data_location_point
    ADD COLUMN speed  double precision,
    ADD COLUMN course double precision;
