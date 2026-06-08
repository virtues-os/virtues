-- The demo life lives in virtues-core/seeds/demo/life.sql.gz (deploy/demo/).
-- This placeholder keeps `virtues seed` and its test working on old branches.
DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'wiki_people' AND column_name = 'name') THEN
    EXECUTE 'INSERT INTO wiki_people (id, name) VALUES (''person_demo_stub'', ''Nick Halden'') ON CONFLICT DO NOTHING';
  ELSE
    EXECUTE 'INSERT INTO wiki_people (id, canonical_name) VALUES (''person_demo_stub'', ''Nick Halden'') ON CONFLICT DO NOTHING';
  END IF;
END $$;
