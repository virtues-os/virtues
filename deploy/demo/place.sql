-- Place the demo life on the calendar. Demo boxes only; fiction throughout.
--
-- The life (virtues-core/seeds/demo/life.sql.gz) is two kinds of bucket, listed
-- row by row in demo.rows:
--
--   0..6  the showcase week, Saturday..Friday. Each day lives on the most
--         recent date <= today with its own weekday: weekdays are always
--         true, today is always a full day, and a date keeps its content for
--         a week. A day moves only when its weekday comes round again.
--   -1    the three-year backbone. It ends the day before the week's oldest
--         day, i.e. today - 7, and moves by exact days.
--
-- The week moves first, so the date the backbone lands on next is always
-- free. Membership, not id prefixes, decides what moves: the box's own rows
-- (real weather, a visitor's chat) never do.
--
-- "today" is the box's UTC date: a viewer in Europe or the US always finds
-- their own today inside the window. Run hourly; a run with nothing to move
-- returns without writing.

CREATE OR REPLACE FUNCTION demo.move(b smallint, shift integer) RETURNS void
LANGUAGE plpgsql AS $fn$
DECLARE
  t record;
  cols text;
  c record;
  n bigint;
BEGIN
  IF shift = 0 THEN RETURN; END IF;

  -- Day rows are unique per date. Park ours out of the way, clear whatever the
  -- box made for itself on the landing dates (an empty row for today, or a
  -- day it wrote from nothing), then land.
  UPDATE wiki_days SET date = date + 100000
   WHERE id IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_days' AND bucket = b);
  DELETE FROM wiki_days w
   WHERE w.date IN (SELECT date - 100000 + shift FROM wiki_days
                     WHERE id IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_days' AND bucket = b))
     AND NOT EXISTS (SELECT 1 FROM demo.rows r WHERE r.tbl = 'wiki_days' AND r.id = w.id);
  GET DIAGNOSTICS n = ROW_COUNT;
  IF n > 0 THEN RAISE NOTICE 'demo: cleared % day row(s) the box made on landing dates', n; END IF;

  FOR t IN SELECT DISTINCT tbl FROM demo.rows
            WHERE bucket = b AND tbl NOT IN ('wiki_days', 'wiki_chapters') LOOP
    -- When the life happened. Write times (created_at, narrated_at,
    -- last_written_at) stay, except on the rows whose creation IS the event:
    -- a seeded chat or applet run is dated by when it was written.
    SELECT string_agg(format('%I = %I + make_interval(days => %s)', column_name, column_name, shift), ', ')
      INTO cols
      FROM information_schema.columns
     WHERE table_schema = 'public' AND table_name = t.tbl
       AND (column_name IN ('started_at', 'ended_at', 'occurred_at', 'issued_at', 'sunrise', 'sunset',
                            'authorized_timestamp', 'last_modified_time')
            OR (t.tbl IN ('app_chats', 'app_chat_messages', 'app_applet_runs')
                AND column_name IN ('created_at', 'updated_at')));
    CONTINUE WHEN cols IS NULL;
    EXECUTE format('UPDATE %I SET %s WHERE id::text IN (SELECT id FROM demo.rows WHERE tbl = %L AND bucket = %s)',
                   t.tbl, cols, t.tbl, b);
  END LOOP;

  -- Chapters may not overlap (an exclusion constraint checked row by row), so
  -- move them one at a time, leading edge first.
  FOR c IN SELECT id FROM wiki_chapters
            WHERE id IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_chapters' AND bucket = b)
            ORDER BY CASE WHEN shift > 0 THEN started_at END DESC NULLS LAST,
                     CASE WHEN shift < 0 THEN started_at END ASC NULLS LAST LOOP
    UPDATE wiki_chapters SET started_at = started_at + shift, ended_at = ended_at + shift WHERE id = c.id;
  END LOOP;

  UPDATE wiki_days SET date = date - 100000 + shift
   WHERE id IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_days' AND bucket = b);

  -- A day page's title is its date.
  UPDATE app_pages p SET title = trim(to_char(d.date, 'FMDD FMMonth YYYY'))
    FROM wiki_articles a JOIN wiki_days d ON d.id = a.subject_id
   WHERE a.page_id = p.id AND a.subject_type = 'day'
     AND d.id IN (SELECT id FROM demo.rows WHERE tbl = 'wiki_days' AND bucket = b);

  UPDATE demo.buckets SET landed = landed + shift WHERE bucket = b;
END $fn$;

DO $$
DECLARE
  k record;
  want date;
  shift integer;
BEGIN
  FOR k IN SELECT bucket, landed FROM demo.buckets WHERE bucket >= 0 ORDER BY bucket LOOP
    -- bucket 0 is Saturday; Postgres dow counts Sunday = 0.
    want := current_date - ((extract(dow FROM current_date)::int - ((k.bucket + 6) % 7) + 7) % 7);
    IF want <> k.landed THEN
      PERFORM demo.move(k.bucket, want - k.landed);
      RAISE NOTICE 'demo: week day % now on %', k.bucket, want;
    END IF;
  END LOOP;

  SELECT coalesce(min(landed) - 1, current_date) INTO want FROM demo.buckets WHERE bucket >= 0;
  SELECT want - landed INTO shift FROM demo.buckets WHERE bucket = -1;
  IF shift <> 0 THEN
    PERFORM demo.move(-1::smallint, shift);
    RAISE NOTICE 'demo: backbone now ends %', want;
  END IF;
END $$;
