-- Find a tombstone from a raw record's own id.
--
-- When a sync writes a raw file, the lake drops any record that has a
-- tombstone (`storage::raw_copies`). A raw record names itself by its
-- provider's id, and a row's `source_stream_id` is that id or ends with it
-- after a colon (`github:star:<node id>`, `<calendar id>:<event id>`). So the
-- lookup is by the text after the last colon, and the primary key
-- (source_table, source_stream_id) cannot serve it.
--
-- The expression here and the one in `raw_copies::tombstoned_among` must be
-- the same text, or Postgres does not use this index.
CREATE INDEX idx_app_tombstones_id_tail
    ON app_tombstones ((substring(source_stream_id from '[^:]*$')));
