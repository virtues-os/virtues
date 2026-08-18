-- One name for "when this happened": `occurred_at`.
--
-- The schema had SEVEN names for the same idea — `timestamp` (16 tables),
-- `start_time` (7), plus `played_at`, `arrival_time`, `valid_time` and
-- `created_time` one apiece — and `occurred_at`, which `app_auth_event` was
-- already using correctly. So the good name was house style in one corner and
-- unknown everywhere else.
--
-- ## Why this is worth a rename rather than a note in a style guide
--
-- The inconsistency had already grown architecture around itself.
-- `OntologyDescriptor` carries a `timestamp_sql` field — a per-ontology SQL
-- expression, spliced into generated queries — holding FIVE different
-- expressions across thirteen descriptors, for no reason except that the column
-- has five names. A schema defect that makes you add a configuration field to
-- paper over it is not cosmetic; it is a small piece of the system existing to
-- apologize for a naming decision. With one name, that field is a constant, and
-- the next person adding an ontology does not have to be told which of five
-- spellings this table uses.
--
-- `timestamp` is also a Postgres keyword, so every hand-written query and every
-- pg_dump had to quote it. That quoting is now gone from the whole schema.
--
-- ## The rule, so this does not regrow
--
--   * `occurred_at` — when the thing happened. One instant.
--   * `started_at` / `ended_at` — when a thing that has duration began and
--     ended.
--   * `created_at` / `updated_at` — when WE wrote the row. Never confuse these
--     with when the event happened; that conflation is what produced
--     `created_time` sitting next to `created_at` on the same table.
--
-- `data_location_visit` moves under the second rule, not the first: a visit has
-- an `arrival_time` AND a `departure_time`, which makes it a span wearing two
-- bespoke names. It becomes `started_at`/`ended_at` like every other span.
--
-- The remaining `start_time`/`end_time` pairs are left for a separate pass —
-- they are already internally consistent and unambiguous, so they are style
-- rather than the confusion this migration is about.

-- ── The sixteen `timestamp` columns ─────────────────────────────────────────
ALTER TABLE data_activity_web_browsing RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_communication_email RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_communication_message RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_content_bookmark RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_content_conversation RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_financial_asset RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_financial_liability RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_financial_transaction RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_health_active_energy RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_health_distance RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_health_heart_rate RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_health_hrv RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_health_steps RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE data_location_point RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE search_embeddings RENAME COLUMN "timestamp" TO occurred_at;
ALTER TABLE wiki_refs RENAME COLUMN "timestamp" TO occurred_at;

-- ── The four bespoke instants ───────────────────────────────────────────────
ALTER TABLE data_activity_listening   RENAME COLUMN played_at    TO occurred_at;
ALTER TABLE data_content_document     RENAME COLUMN created_time TO occurred_at;
ALTER TABLE data_environment_weather  RENAME COLUMN valid_time   TO occurred_at;

-- ── The span that was wearing bespoke names ─────────────────────────────────
ALTER TABLE data_location_visit RENAME COLUMN arrival_time   TO started_at;
ALTER TABLE data_location_visit RENAME COLUMN departure_time TO ended_at;
