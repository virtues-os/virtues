-- Move the seeded demo life so its best day is today, instead of in February 2026.
--
-- The other three seed files are written with absolute dates, deliberately:
-- absolute SQL is readable, diffable, and re-generatable, and every date in
-- them is load-bearing relative to the others (a workout the morning after a
-- bad night's sleep, a transaction at the place the location trace says he
-- was). Rewriting 1,771 literals into relative expressions would destroy that
-- readability to buy nothing this file cannot buy in one pass.
--
-- What it cost to not have this: `demo_narrative.sql` ends 2026-02-12 and
-- `demo_day.sql` covers Feb 12-14. Home asks for the browser's literal today
-- (`HomeView.svelte`, `todayDate = ymd(_now)`) and has NO fallback to "the
-- newest day that has data" -- so a box seeded any later than about February
-- opens onto nothing at all, with the newest data months stale. That is what an
-- App Store reviewer would have met through the demo box, and what a developer
-- has been meeting since February (agents/build/review-access-plan.md).
--
-- ANCHOR ON THE RICHEST DAY, NOT THE LAST ONE. The 12-week narrative is almost
-- entirely `wiki_events` -- 741 rows of five-to-eleven a day, and essentially no
-- raw streams. All of the instrumentation lives in ONE day out of
-- `demo_day.sql`: on a fresh seed that day holds every location point, every
-- message, the sleep and the steps and the transaction, and every other day in
-- the set holds none of them. So "which day is today" is the entire question,
-- and moving the LAST day onto today leaves the good day just off the edge of
-- the only page anyone opens.
--
-- WHY NOT WHOLE WEEKS. Shifting by a multiple of 7 would preserve weekday shape
-- -- commutes on weekdays, the long run on a Saturday, the instrumented day
-- authored as a Friday. It cannot also put that day on today, because today is
-- whatever weekday it happens to be. Tried it first: it landed the one rich day
-- on tomorrow and left the reviewer on a thin Thursday, which is the same empty
-- page in a better disguise. Weekday shape is worth less than the demo working,
-- so the shift is exact and the weekdays rotate. If the narrative ever grows
-- real per-day streams, revisit this -- the trade changes.
--
-- IDEMPOTENT BY CONSTRUCTION. The anchor is read from the data itself, not from
-- a constant, so once the richest day IS today the shift computes to zero and
-- nothing moves. That also makes re-running the seeder the way to re-age a
-- long-lived demo box: it walks the whole life forward to today.

DO $$
DECLARE
    cur_anchor  date;
    shift_days  int;
    park_days   CONSTANT int := 100000;   -- see the wiki_days note below
    r           record;
BEGIN
    -- The instrumented day, found by where the raw observations actually are
    -- rather than by a hardcoded date that would rot the moment the seed is
    -- regenerated. Ties break toward the later day.
    SELECT occurred_at::date INTO cur_anchor
    FROM data_location_point
    GROUP BY 1
    ORDER BY count(*) DESC, 1 DESC
    LIMIT 1;

    -- A seed with no location trace at all (or a partial one) still deserves to
    -- land near today; fall back to the last narrated day.
    IF cur_anchor IS NULL THEN
        SELECT max(date) INTO cur_anchor FROM wiki_days;
    END IF;

    IF cur_anchor IS NULL THEN
        RAISE NOTICE 'demo re-anchor: no demo data present -- nothing to move';
        RETURN;
    END IF;

    shift_days := current_date - cur_anchor;

    IF shift_days = 0 THEN
        RAISE NOTICE 'demo re-anchor: the richest day is already today (%) -- nothing to move',
            cur_anchor;
        RETURN;
    END IF;

    -- `wiki_days.date` carries a UNIQUE index, and Postgres inserts into a
    -- unique index row by row within an UPDATE rather than deferring the check
    -- to the end of the statement. So a uniform `date = date + N` collides
    -- mid-statement whenever N is smaller than the span of the data -- an
    -- everyday case here, since re-ageing an already-shifted box moves it by
    -- days against a twelve-week span. Parking the whole column far outside any
    -- plausible range and landing it from there means neither pass ever writes a
    -- value the column already holds. Everything else on these tables is
    -- non-unique and moves in one shot.
    UPDATE wiki_days SET date = date + park_days;
    UPDATE wiki_days SET date = date - park_days + shift_days;

    FOR r IN
        SELECT c.table_name, c.column_name, c.data_type
        FROM information_schema.columns c
        JOIN information_schema.tables t
          ON t.table_schema = c.table_schema
         AND t.table_name   = c.table_name
        WHERE c.table_schema = 'public'
          AND t.table_type   = 'BASE TABLE'
          -- The seeded namespaces. `app_*` is product state the seed does not
          -- write, and moving it would drag pairing and audit rows with it.
          AND (c.table_name LIKE 'data\_%' OR c.table_name LIKE 'wiki\_%')
          AND c.data_type IN ('timestamp with time zone',
                              'timestamp without time zone',
                              'date')
          AND c.is_generated = 'NEVER'
          -- `created_at`/`updated_at` are when WE wrote the row, never when the
          -- thing happened (CLAUDE.md, column naming). No seed file sets either,
          -- so both already default to now() -- correct as they stand, and
          -- shifting them forward would date the rows into the future.
          AND c.column_name NOT IN ('created_at', 'updated_at')
          -- A birthday is a biographical fact, not a point on the timeline:
          -- moving it changes how old someone is. No current seed sets one; the
          -- exclusion is here so that adding one later cannot go wrong quietly.
          AND NOT (c.table_name = 'wiki_people' AND c.column_name = 'birthday')
          -- Already handled above, and it must not be moved twice.
          AND NOT (c.table_name = 'wiki_days' AND c.column_name = 'date')
        ORDER BY c.table_name, c.column_name
    LOOP
        IF r.data_type = 'date' THEN
            -- date + int stays a date; date + interval would widen to timestamp
            -- and then need an assignment cast back.
            EXECUTE format(
                'UPDATE public.%I SET %I = %I + $1 WHERE %I IS NOT NULL',
                r.table_name, r.column_name, r.column_name, r.column_name
            ) USING shift_days;
        ELSE
            EXECUTE format(
                'UPDATE public.%I SET %I = %I + $1 WHERE %I IS NOT NULL',
                r.table_name, r.column_name, r.column_name, r.column_name
            ) USING make_interval(days => shift_days);
        END IF;
    END LOOP;

    RAISE NOTICE 'demo re-anchor: moved % days; the instrumented day is now %',
        shift_days, cur_anchor + shift_days;
END $$;
