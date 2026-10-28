-- 0067_add_applet_lake_kind
--
-- A file an applet's sandboxed code wrote to its own lake folder,
-- lake/applets/<name>/, which the runner indexes after each run. Keyed by
-- storage_key like media; provider is 'applet' and stream_name the applet id.
ALTER TABLE lake_objects DROP CONSTRAINT IF EXISTS lake_objects_kind_check;
ALTER TABLE lake_objects ADD CONSTRAINT lake_objects_kind_check
    CHECK (kind IN ('raw_stream', 'media', 'applet'));
