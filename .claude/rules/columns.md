---
paths:
  - "**/*.sql"
  - "virtues-core/src/**/*.rs"
  - "crates/**/*.rs"
  - "applets/**/*.rs"
---

# Column naming

Loaded because you are in SQL or in Rust that talks to it. This schema is
shown to a model at runtime and drives a table-driven UI, so a column name is
an interface, not a label.

Renaming 21 columns on 2026-08-17/18 established these. The schema had **seven**
names for "when this happened" and had grown a configuration field to paper over
it.

- `occurred_at` — an instant. When the thing happened.
- `started_at` / `ended_at` — a span.
- `created_at` / `updated_at` — when WE wrote the row. Never the event; that
  conflation is what produced `created_time` sitting beside `created_at`.
- `is_` / `has_` for booleans; no bare adjectives (`active` → `is_active`).
- A unit suffix on every quantity: `_cents`, `_ms`, `_bytes`, `_meters`.
- Prefixes stay: `app_` product state, `data_` ingested, `wiki_` derived,
  `search_` indexes. Not decoration — this schema is shown to an LLM at runtime
  and drives a table-driven UI, so the prefix is a namespace the model matches.
- `data_*` singular (one observation); everything else plural.

**Renames the compiler cannot check.** `sqlx::query` is untyped, so a renamed
column breaks at runtime, not build time. When you rename one, sweep: SQL
strings, `row.get("…")` accessors **including nested generics like
`::<DateTime<Utc>, _>`** (a `[^>]*` regex silently misses those), the
`sql_query.rs` catalog the model reads, and the registry's `timestamp_column`.
Leave alone: JSON payload keys and tool arguments that merely share a word with
a column.
