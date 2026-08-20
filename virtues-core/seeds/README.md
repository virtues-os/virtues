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
  Feb 11 2026) used as the novelty-scoring baseline. ~638 INSERTs, ~8 KLOC.
  Originally split into four week-range files; consolidated here for the
  Postgres cutover.

- **`demo_bookmarks.sql`** — the same designer's saves (11 rows), covering
  every enrichment state the `/bookmarks` room renders: enriched, pending,
  held-for-the-image-pass, and tombstoned-but-kept. Deliberately not uniformly
  happy — a room built against all-enriched data hides its own empty states.
  `extraction_text` mirrors `ExtractionRecord::to_embed_text`, so if that
  rendering changes this file should follow it.

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
