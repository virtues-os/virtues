#!/bin/sh
# Export the frozen demo week as one self-contained SQL file. Run on the demo box:
#   sudo -u postgres sh export.sh > demo-week.sql
# One statement per table: INSERT (cols) SELECT cols FROM json_populate_recordset(...),
# so column order never matters and a re-load is idempotent.
set -e
P="psql -X -At -v ON_ERROR_STOP=1 -d virtues"
one() {  # table, where-clause. Generated columns are left for Postgres to compute.
  cols=$($P -c "SELECT string_agg(quote_ident(column_name), ', ' ORDER BY ordinal_position) FROM information_schema.columns WHERE (table_schema || '.' || table_name = '$1' OR (table_schema = 'public' AND table_name = '$1')) AND is_generated = 'NEVER'")
  $P -c "SELECT format('INSERT INTO %s (%s) SELECT %s FROM json_populate_recordset(NULL::%s, %L) ON CONFLICT DO NOTHING;', '$1', '$cols', '$cols', '$1', json_agg(to_jsonb(seed_row))) FROM $1 seed_row WHERE $2 HAVING count(*) > 0"
}
echo "-- Demo week (fiction): the demo persona's showcase week, frozen $(date -u +%F)."
echo "-- Load on a demo box after the demo3y seed, then run ring.sql to place it."
echo "BEGIN;"
echo "CREATE SCHEMA IF NOT EXISTS demo_week;"
echo "CREATE TABLE IF NOT EXISTS demo_week.days (day smallint PRIMARY KEY, landed date NOT NULL);"
echo "CREATE TABLE IF NOT EXISTS demo_week.rows (tbl text NOT NULL, id text NOT NULL, day smallint NOT NULL, PRIMARY KEY (tbl, id));"
one wiki_places "id LIKE 'wk!_%' ESCAPE '!'"
one wiki_people "id = 'wk_self'"
# Entities the box derived from the week (merchants from its purchases, ...):
# the week's refs point at them, so they travel with it.
W="SELECT entity_id FROM wiki_refs WHERE id IN (SELECT id FROM demo_week.rows WHERE tbl = 'wiki_refs')"
one wiki_orgs "id IN ($W) AND id ~ '^org_[0-9a-f]{16}$'"
one wiki_people "id IN ($W) AND id ~ '^person_[0-9a-f]{16}$'"
one wiki_places "id IN ($W) AND id ~ '^place_[0-9a-f]{16}$'"
one wiki_days "id IN (SELECT id FROM demo_week.rows WHERE tbl = 'wiki_days')"
for t in $($P -c "SELECT DISTINCT tbl FROM demo_week.rows WHERE tbl <> 'wiki_days' ORDER BY 1"); do
  one "$t" "id::text IN (SELECT id FROM demo_week.rows WHERE tbl = '$t')"
done
# The day pages the box wrote, and the articles that point at them.
one app_pages "id IN (SELECT page_id FROM wiki_articles WHERE subject_type = 'day' AND subject_id IN (SELECT id FROM demo_week.rows WHERE tbl = 'wiki_days'))"
one wiki_articles "subject_type = 'day' AND subject_id IN (SELECT id FROM demo_week.rows WHERE tbl = 'wiki_days')"
one demo_week.days "true"
one demo_week.rows "true"
echo "COMMIT;"
