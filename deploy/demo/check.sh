#!/bin/sh
# CI: the demo life still loads on the current migrations and places cleanly.
# Everything runs in one transaction that is rolled back, so it is safe against
# any database that has the migrations applied.
#
#   sh deploy/demo/check.sh "$DATABASE_URL"
#
# When a migration renames or drops a column the life uses, this fails: rebuild
# the file with `python3 tools/demo/build.py` and commit it with the migration.
set -eu
URL=${1:-${DATABASE_URL:?}}
HERE=$(cd "$(dirname "$0")" && pwd)
LIFE="$HERE/../../virtues-core/seeds/demo/life.sql.gz"

out=$({
  echo "BEGIN;"
  gunzip -c "$LIFE"
  cat "$HERE/place.sql"
  echo "\\echo SECOND_PLACE"
  cat "$HERE/place.sql"
  cat <<'SQL'
DO $$
DECLARE t text; missing bigint; total bigint := 0;
BEGIN
  FOR t IN SELECT DISTINCT tbl FROM demo.rows LOOP
    EXECUTE format('SELECT count(*) FROM demo.rows r WHERE r.tbl = %L AND NOT EXISTS (SELECT 1 FROM %I x WHERE x.id::text = r.id)', t, t)
      INTO missing;
    IF missing > 0 THEN RAISE EXCEPTION 'demo: % rows listed for % are not there', missing, t; END IF;
  END LOOP;
  IF NOT EXISTS (SELECT 1 FROM wiki_events e JOIN wiki_days d ON d.id = e.day_id WHERE d.date = current_date) THEN
    RAISE EXCEPTION 'demo: today has no events after placement';
  END IF;
  IF (SELECT landed FROM demo.buckets WHERE bucket = -1) <> (SELECT min(landed) - 1 FROM demo.buckets WHERE bucket >= 0) THEN
    RAISE EXCEPTION 'demo: the backbone does not end the day before the week';
  END IF;
  SELECT count(*) INTO total FROM demo.rows;
  RAISE NOTICE 'demo life OK: % rows', total;
END $$;
SQL
  echo "ROLLBACK;"
} | psql "$URL" -X -q -v ON_ERROR_STOP=1 -f - 2>&1)
echo "$out" | tail -3
if echo "$out" | sed -n '/SECOND_PLACE/,$p' | grep -q 'demo: week day\|demo: backbone now'; then
  echo "✖ a second placement moved rows: place.sql is not idempotent" >&2; exit 1
fi
echo "$out" | grep -q 'demo life OK' || { echo "✖ demo life check failed" >&2; exit 1; }
echo "✓ demo life loads and places"
