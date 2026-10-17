-- Freeze the demo week's membership (demo box only). Run once, after the box has
-- derived the week's events and pages. Lists every row that belongs to the week
-- and which of its seven days it lives on; ring.sql moves exactly these rows.
--
-- psql -v week_start=YYYY-MM-DD : the date day 0 (Saturday) currently sits on.
-- data_* rows count only when they are ours (`wk_` ids): the box writes real
-- weather into the same dates and that must stay on its own clock. The exception
-- is data_audio_session, which the box rebuilds from the week's audio under its
-- own ids. wiki_* rows in the window are the box's derivation of the week, so
-- they all count.

CREATE SCHEMA IF NOT EXISTS demo_week;
CREATE TABLE IF NOT EXISTS demo_week.days (day smallint PRIMARY KEY, landed date NOT NULL);
CREATE TABLE IF NOT EXISTS demo_week.rows (tbl text NOT NULL, id text NOT NULL, day smallint NOT NULL,
                                           PRIMARY KEY (tbl, id));
TRUNCATE demo_week.days, demo_week.rows;
INSERT INTO demo_week.days SELECT g, :'week_start'::date + g FROM generate_series(0, 6) g;

DO $$
DECLARE
  ws date := (SELECT landed FROM demo_week.days WHERE day = 0);
  t record;
  col text;
  dexpr text;
BEGIN
  FOR t IN
    SELECT c.table_name
      FROM information_schema.columns c
      JOIN information_schema.tables tb ON tb.table_name = c.table_name AND tb.table_schema = 'public'
                                        AND tb.table_type = 'BASE TABLE'
     WHERE c.table_schema = 'public' AND c.column_name = 'id'
       AND (c.table_name LIKE 'data\_%' OR c.table_name LIKE 'wiki\_%')
       AND c.table_name NOT IN ('wiki_articles', 'wiki_places', 'wiki_people', 'wiki_orgs', 'wiki_chapters',
                                'wiki_stories', 'wiki_years', 'wiki_rules', 'wiki_notes')
  LOOP
    col := NULL;
    SELECT column_name INTO col FROM information_schema.columns
     WHERE table_schema = 'public' AND table_name = t.table_name
       AND column_name IN ('started_at', 'occurred_at', 'date')
     ORDER BY array_position(ARRAY['date', 'started_at', 'occurred_at'], column_name::text) LIMIT 1;
    CONTINUE WHEN col IS NULL;
    dexpr := CASE WHEN col = 'date' THEN quote_ident(col)
                  ELSE format('(%I AT TIME ZONE %L)::date', col, 'America/Chicago') END;
    EXECUTE format(
      'INSERT INTO demo_week.rows (tbl, id, day)
       SELECT %L, id::text, least(6, greatest(0, %s - %L::date))::smallint
         FROM %I
        WHERE %s BETWEEN %L::date - %s AND %L::date + 6 AND %s
       ON CONFLICT DO NOTHING',
      t.table_name, dexpr, ws, t.table_name, dexpr, ws,
      CASE WHEN t.table_name LIKE 'data\_%' THEN 1 ELSE 0 END, ws,
      CASE WHEN t.table_name = 'data_audio_session' THEN 'true'   -- the box re-sessionizes the week's audio
           WHEN t.table_name LIKE 'data\_%' THEN 'id::text LIKE ''wk!_%'' ESCAPE ''!''' ELSE 'true' END);
  END LOOP;
END $$;

SELECT tbl, count(*), min(day), max(day) FROM demo_week.rows GROUP BY 1 ORDER BY 1;
