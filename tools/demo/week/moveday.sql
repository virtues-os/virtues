-- Move every row of one local day from :src to :dst (same weekday), demo box only.
-- data_* rows: ours only (wk_ ids). wiki_* rows: all rows on that day (the box's
-- derivation of it). Columns moved: the life-time ones, as in ring.sql.
SELECT set_config('demo.src', :'src', false), set_config('demo.dst', :'dst', false);
DO $$
DECLARE
  src date := current_setting('demo.src')::date;
  dst date := current_setting('demo.dst')::date;
  shift int := dst - src;
  t record; col text; dexpr text; cols text;
BEGIN
  DELETE FROM wiki_days w WHERE w.date = dst AND NOT EXISTS (SELECT 1 FROM wiki_events e WHERE e.day_id = w.id);
  FOR t IN SELECT c.table_name FROM information_schema.columns c
             JOIN information_schema.tables tb ON tb.table_name = c.table_name AND tb.table_schema='public' AND tb.table_type='BASE TABLE'
            WHERE c.table_schema='public' AND c.column_name='id'
              AND (c.table_name LIKE 'data\_%' OR c.table_name LIKE 'wiki\_%')
              AND c.table_name NOT IN ('wiki_articles','wiki_places','wiki_people','wiki_orgs','wiki_chapters','wiki_stories','wiki_years','wiki_rules','wiki_notes')
  LOOP
    col := NULL;
    SELECT column_name INTO col FROM information_schema.columns
     WHERE table_schema='public' AND table_name=t.table_name AND column_name IN ('started_at','occurred_at','date')
     ORDER BY array_position(ARRAY['date','started_at','occurred_at'], column_name::text) LIMIT 1;
    CONTINUE WHEN col IS NULL;
    dexpr := CASE WHEN col='date' THEN quote_ident(col) ELSE format('(%I AT TIME ZONE %L)::date', col, 'America/Chicago') END;
    SELECT string_agg(format('%I = %I + make_interval(days => %s)', column_name, column_name, shift), ', ') INTO cols
      FROM information_schema.columns
     WHERE table_schema='public' AND table_name=t.table_name
       AND column_name IN ('started_at','ended_at','occurred_at','date','authorized_timestamp','last_modified_time','start_time','end_time');
    EXECUTE format('UPDATE %I SET %s WHERE %s = %L AND %s', t.table_name, cols, dexpr, src,
                   CASE WHEN t.table_name LIKE 'data\_%' THEN 'id::text LIKE ''wk!_%'' ESCAPE ''!''' ELSE 'true' END);
  END LOOP;
END $$;
