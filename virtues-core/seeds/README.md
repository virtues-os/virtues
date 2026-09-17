# Seeds

Three seed SQL files for the appliance. All run via `sqlx::raw_sql` from
[core/src/seeding/](../src/seeding/) and are idempotent — every `INSERT`
ends with `ON CONFLICT DO NOTHING` so a re-run is a no-op.

## Files

- **`prod_minimum.sql`** *(seeded by Rust at every boot)* — system actions
  and required defaults. Inserted from
  [`prod_seed.rs`](../src/seeding/prod_seed.rs) as parameterised queries,
  not a static `.sql` file. Listed here for completeness.

- **`demo_day.sql`** — a single richly-instrumented day (Friday, Feb 13 2026
  + flanking days) used for UI demos and as primary test data. ~80 INSERTs,
  ~1.4 KLOC.

- **`demo_narrative.sql`** — the 12-week character narrative (Nov 24 2025 →
  Feb 11 2026 as written; see `demo_reanchor.sql`, which moves it to the
  current week at seed time) used as the novelty-scoring baseline. ~638 INSERTs, ~8 KLOC.
  Originally split into four week-range files; consolidated here for the
  Postgres cutover.

- **`demo_bookmarks.sql`** — the same designer's saves (11 rows), covering
  every enrichment state the `/bookmarks` room renders: enriched, pending,
  held-for-the-image-pass, and tombstoned-but-kept. Deliberately not uniformly
  happy — a room built against all-enriched data hides its own empty states.
  `extraction_text` mirrors `ExtractionRecord::to_embed_text`, so if that
  rendering changes this file should follow it.

- **`demo3y/`** *(generated, gitignored)* — three years of a life, emitted by
  [`tools/gen-demo-seed.py`](../../tools/gen-demo-seed.py). Where
  `demo_narrative.sql` is 80 days of derived events with **no raw data beneath
  them** — so every provenance surface (the dayline, sources, the deck's lanes,
  the movement map, every chart) is empty on 79 of its 80 days — this is built
  bottom-up: ~24k raw stream rows across health, location, messages, email,
  finance, calendar and weather, then the derived layer on top, then the
  creation layer (chats, pages, articles) that nothing seeded before. Nine
  weeks also cannot populate Chapters, Years, Stories or Lifeline; three years
  can.

  Run it with `seeds/demo3y/run.sh`, which ends in its own re-anchor — **not**
  `demo_reanchor.sql`, which would shift the set a second time. Regenerate with
  `python3 tools/gen-demo-seed.py --check-db <db>`; the check validates every
  column against a live database before writing, because a seed is raw SQL and
  a renamed column is a runtime failure on a box months later rather than a
  compile error anywhere.

  **Derived columns are deliberately left NULL** — `novelty_z`,
  `local_novelty_z`, `avg_hr`, `embedding`. `compute_novelty_for_day` only
  selects events where a novelty channel is NULL, so seeding a score makes the
  scorer skip that event forever and the row keeps an invented number. Let
  `virtues reindex` fill them.

- **`demo_reanchor.sql`** *(runs last, after the three above)* — walks the
  whole seeded life forward so that **the instrumented day lands on today**,
  rather than in February 2026. The other files keep their absolute dates,
  which is what makes them readable and re-generatable; this is where "relative
  to today" is bought, once, in one pass. It anchors on the day carrying the
  most location points — all the raw streams live in one day, and Home asks for
  the literal current date with no "newest day with data" fallback, so any
  other anchor leaves the good day just off the only page anyone opens. The
  shift is exact rather than a whole number of weeks, so weekdays rotate; the
  file explains why that trade was made and when to revisit it. Idempotent —
  the anchor comes out of the data, so a second run moves nothing, and
  re-running the seeder is therefore also how you re-age a long-lived demo box.

## Why these are different from migrations

Migrations (under [core/migrations/](../migrations/)) describe schema.
Seeds describe data. The migration runner (sqlx) tracks which migrations
have run; seeds are run explicitly by the prod or demo seeder Rust code.

## Re-running

Both files are safe to re-run against a populated database — `ON CONFLICT
DO NOTHING` skips any rows already present. To wipe and reseed:

```sh
docker exec virtues-pg psql -U virtues -d virtues -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
cargo run -p virtues   # re-runs migrations + prod_minimum
cargo run -p virtues --bin virtues-prod-seed -- --demo   # adds demo_day + demo_narrative
```

## Generating new seeds

If you regenerate the narrative (e.g. updating the character profile),
emit Postgres-compatible SQL directly:
- Use `'{...}'::jsonb` for JSONB columns (or rely on pg's implicit
  text→jsonb coercion when the column is JSONB)
- Use `TRUE` / `FALSE` for BOOLEAN columns, not `1` / `0`
- Use `'2026-02-13T12:30:00Z'` ISO 8601 strings for TIMESTAMPTZ
- Always append ` ON CONFLICT DO NOTHING` to every `INSERT`
- Keep writing absolute dates; `demo_reanchor.sql` moves the whole set to
  today at seed time. Keep the raw streams concentrated in one instrumented
  day, or say so here — that day is what the re-anchor pass aims at today, and
  spreading the streams thinly across the narrative would make a whole-week
  shift (weekday-preserving) the better rule instead.
