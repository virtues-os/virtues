# Schema audit — every table, every field (2026-08-28)

*Four parallel audits (app_\*, data_\*, wiki_\*/search_\*, infra + the chain)
over the 82 tables of the post-squash schema, each column checked against
actual reads and writes in the code, with row counts from a fielded box.
Verdicts carry file:line evidence; this doc is the consolidated result.
Nothing here has been fixed yet — this is the map, not the surgery.*

## A. Broken right now (fix before any cleanup)

1. **Person and place article generation fails hard.**
   `entity_article_gen.rs:216,245` SELECT `ref_count` — a column that does
   not exist on `wiki_people`/`wiki_places` (the 2026-08-18 rename went to
   `seen_count`; these two sites are raw `sqlx::query`, so the compile-time
   sweep missed them; the org branch was fixed). `fetch_one` + `?` → hard
   error. Box shows 41 articles against 196 orgs + 573 people.
   `sql_query.rs:252,258,264` also advertises the phantom `ref_count` to
   the model for all three tables.
2. **`morning_examen` has failed quietly since 2026-08-17.**
   `applets/morning_examen/manifest.toml:36-42` hands the model the
   pre-rename column names (`start_time`, `end_time`, `timestamp`) and
   instructs "These column names are exact." Every run since the rename
   issues SQL against columns that don't exist.
3. **Date-scoped document search is chronologically wrong.**
   `extracted_document_chunks.created_at` (extraction time) is wired as the
   ontology's event-time (`ontologies.rs:1388,1401`) — a 2019 PDF uploaded
   today sorts as a 2026 document. Needs a real `occurred_at` sourced from
   the file.
4. **Dayline web section is ordered by an always-NULL column.**
   `dayline/context.rs:227` does `ORDER BY visit_duration_seconds DESC`;
   no collector writes that column.
5. **`app_drive_usage` is a singleton that is never seeded** — if any
   write path is `UPDATE` (not upsert), drive usage is silently always
   zero. Verify the write path.
6. **The Usage grid shows "ok" for every AI call** including failures:
   `app_ai_calls.status` is never written (constant default), yet
   selected and rendered (`ai_calls.rs:167,240`).
7. **The API usage limiter can never fire.** `app_usage_limits` is seeded
   at boot and read, but `app_api_usage` (the counter it compares against)
   is written from only 3 call sites and holds 0 rows — `check_limit`
   always sees 0. Wire it or delete the subsystem (see D).
8. **`credentials.last_seen_at` lies in the UI** — only bumped on token
   refresh, so api_key/Plaid credentials in daily use render "never seen."
9. **The model-facing catalog advertises columns nothing writes**:
   `ref_count`, `url` (app_session), `event_type`/`conference_url`
   (calendar), `content_type` (bookmark), `model` (conversation),
   `credit_limit`/`is_active` (account), `visit_duration_seconds`
   (browsing). And it instructs `JOIN wiki_day_prose` while
   `get_schema("wiki_day_prose")` is refused (views excluded from the
   fence at `sql_query.rs:367,447`).
10. **Ghost-column selects silently null**: `api/wiki.rs:148-153,969-970`
    still select the squashed `act_id`/`chapter_id` via
    `try_get(...).ok().flatten()` — the idiom converts schema drift into
    permanent silent `None` (~20 uses across wiki.rs and
    entity_article_gen.rs). It is why bug #1 was the *loud* variant.

## B. Dead — drop outright (zero risk)

| what | evidence |
|---|---|
| `app_mcp_servers` + `app_mcp_tools` tables **+ ~500 lines of uncompiled Rust** | consumers `api/mcp_client.rs` and `mcp/client.rs` are not declared in any mod.rs — tracked files that never build; 0 rows |
| `wiki_years` | zero references in Rust/TS/Svelte; still carries PK, UNIQUE, trigger. Frontend echo: `types/year.ts` |
| `wiki_narrative_interview` + `api/narrative_interview.rs` + routes + client fns | superseded 2026-08-27 by the chat interview; client fns have zero callers; 0 rows |
| `data_activity_listening` + descriptor + lane + dayline read | no Spotify collector exists anywhere; a blank lifeline lane, blank stream-health row, dead query per dayline build |
| `app_applet_package` | one INSERT (`applet_git_import.rs:113`), zero SELECTs, 0 rows, no FK to `app_applets` |
| `app_auth_event_archive` | byte-clone of `app_auth_event` + `archived_at`; written by the sweeper, read by nothing — retention risk with no query path |

## C. Dead columns and indexes — batch into one cleanup migration

**Never written (or never read), grouped by table:**

- `wiki_days`: `illustration` (bytea, never written), `morning_baseline`,
  `battery_curve`, `segmented_at` (write-only; `narrated_at` is the read
  marker), `dirty_at` + partial index. `readiness_score`/`readiness_details`
  are rendered by the UI but written by nothing — decide writer-or-drop.
- `wiki_events`: `hrv_z` (never written; scoring writes `hr_z` only),
  `lof_raw` (write-only), `user_created` (redundant with `is_user_added`,
  never true), `dirty_at` + partial index. `embedding bytea` is a private
  second embedding store outside search_vectors — flag, larger fix.
- `wiki_refs` (100k+ rows): `confidence`, `resolved_by`, `metadata` —
  no INSERT site writes them, nothing reads them.
- `app_chats`: `trace`, `action_instruction` — zero SQL references.
- `app_applet_runs`: `parent_run_id`, `transform_stage` + index — code
  comments themselves say "NEITHER HAS EVER BEEN WRITTEN" over 190k rows.
- `app_drive_usage`: `quota_bytes` (overridden by disk_stats),
  `data_lake_bytes` (hardcoded 0, drawn as a real UI segment —
  DriveView.svelte:582), `total_bytes`, `last_scan_at`, `last_scan_bytes`,
  `trash_bytes`, `trash_count`, `warning_80/90/100_sent` + the uncalled
  `check_usage_warnings` ladder.
- `app_user_profile`: `crux`, `technology_vision`, `pain_point_primary`,
  `pain_point_secondary`, `excited_features` — discovery-survey residue,
  read by nothing.
- `app_pages.date` (text!) + `get_reflections_for_date` +
  `/api/pages/reflections/:date` + JournalCard — a retired feature wired
  end-to-end returning `[]` forever.
- `app_assistant_profile.embedding_model_id`, `app_auth_user.is_owner`,
  `app_ai_calls.chat_id`, `app_notebook_items.similarity`,
  `app_page_versions.content_preview` (holds a label, not a preview),
  `app_mcp_tools.server_name` (dies with the table).
- `data_calendar_event`: `event_type`, `conference_url`,
  `conference_platform`, `recurrence_rule`, `timezone`, `block_type` +
  `idx_calendar_block_type` — never written (Google returns them; populate
  or drop). `is_sacred` is READ (home.rs:116, wiki.rs:2630) but never
  written — always false; decide writer-or-drop.
- `data_activity_app_session`: `url`, `document_path`, `app_category`.
- `data_activity_web_browsing`: `visit_duration_seconds`,
  `scroll_depth_percent`.
- `data_health_workout.route_geometry`,
  `data_communication_transcription.speaker_segments`,
  `data_financial_account.institution_id`,
  `data_content_bookmark.content_type`,
  `data_content_conversation.{model,tags,metadata}`,
  `data_communication_email.{to_names,cc_emails,bcc_emails}`,
  `data_financial_transaction.authorized_timestamp`,
  `data_audio_recording.audio_format` (constant 'm4a' × 11k rows).
- `search_embeddings.text_hash` (superseded by `doc_hash`),
  `search_index_meta.fingerprint` (never written; real check lives in the
  embedder), `search_index_meta.built_at` (write-only).
- `lake_objects.source_id` (always = provider),
  `backup_archived_file.{size_bytes,archived_at}` (write-only × 119k),
  `storage_volume.{capacity_bytes,free_bytes,probed_at,last_error_at}`
  (probe telemetry thrown away; backup re-statvfs's),
  `storage_volume.roles` + CHECK (provably constant `{'backup'}`, spent on
  four `ANY()` scans), `credentials.scopes` (write-only).

**Unread indexes (pure write cost):** `idx_lake_objects_kind`,
`idx_lake_objects_stream`, `idx_lake_objects_replay` (three on the
highest-write table in the schema), `idx_backup_archived_file_increment`
(redundant with PK, non-sargable predicate), `idx_storage_volume_state`,
`idx_app_applet_runs_parent`, `idx_calendar_block_type`, plus the two
`dirty_at` partials.

## D. Decisions (product calls, not code calls)

1. **`app_api_usage` + `app_usage_limits` + `Tier`** — a full quota
   subsystem enforcing nothing. Wire `record_service_usage` into every
   metered egress, or delete both tables and `check_limit`/`get_all_usage`
   (`app_ai_calls.cost_micros` already does the real accounting).
2. **`wiki_stories`** — complete read path + UI, zero writers, by design
   ("stories are hand-authored and there is no pipeline"). Ship the
   writer or cut the surface.
3. **`app_erasure`** (0003) — deliberately reserved pre-fielding with a
   38-line rationale; sweeper never built. Keep-as-reserved or cut.
4. **Gmail 30-day cold start** — 488 emails vs 171k messages is by design
   (no backfill path). Decide whether backfill is wanted.
5. **`data_health_workout` = 0 rows despite two working writers** — same
   applet fills heart rate/sleep/steps. Check real `stream_ios_healthkit`
   payloads: collector never emits workouts, or the type match fails.
6. **`data_communication_email` recipient columns** — keep only if
   recipient-side entity resolution is planned (sender-only today).

## E. Systemic diseases (why it got out of hand — fix the class)

1. **Aspirational schema.** Columns, CHECK values, and whole subsystems
   built for futures that never arrived: CHECKs admitting `'zstd'`,
   `'drive'`, `'internal'`, `'network'`, `'degraded'`, `'reauth_required'`,
   `'error'`, `'complete'` — none ever produced; the erasure table; the
   quota subsystem; the drive-warning ladder; applet chaining columns.
   Rule going forward: **the migration that adds a column lands in the
   same PR as its writer and its reader.**
2. **Two unguarded catalogs.** `registered_ontologies()` and
   `sql_query.rs::get_table_metadata` are separate hand-maintained lists
   that disagree in both directions, and nothing verifies `key_columns`
   name real, written columns. Add the inverse check to
   `tools/check-dynamic-inserts.py`: every advertised column must exist
   AND appear in some writer. That single test retires the whole
   phantom-column class.
3. **`try_get(...).ok().flatten()`** turns schema drift into silent nulls.
   Every use is a latent bug #1. Replace with typed gets + `?` per the
   swallowed-query rule; the ghost `act_id`/`chapter_id` selects go first.
4. **The rename never finished.** Survivors: bare-adjective booleans
   (`enabled` ×4, `active` vs `is_active` coexisting, `auto_update`,
   `user_hidden`…), `_time`/`timestamp` instants
   (`authorized_timestamp`, `last_modified_time`, `min/max_timestamp`,
   `sampled_at`-as-8th-name-for-when), `completed_at` beside
   `started_at`, `sleep_start/sleep_end` freshly minted in 0005 ten days
   after the rename pass, constraint/sequence names fossilizing dead
   columns (`*_played_at_not_null`, `app_space_items_id_seq`), and
   **floating-point money** (`estimated_cost_usd double precision` ×2)
   against the schema's own `_cents` rule — while 8 bigint-cents money
   columns lack the `_cents` suffix and two catalog descriptions don't
   say cents at all (agents will report holdings 100× too large).
5. **Four bespoke singleton patterns** (fixed-UUID+CHECK ×3 — one missing
   its CHECK; `singleton boolean` PK; a boolean column *named `id`* in
   0004; partial unique index) with inconsistent seeding. Pick one
   (the named `singleton boolean` is the honest one), convert on touch.
6. **Duplicated truth**: `started_at`+`created_at` pairs written from one
   INSERT (`app_applet_runs`, `app_device`), hand-maintained
   `message_count`, legacy+new model-id column pairs resolved by fallback
   chains in three places, the audio triad storing the same words three
   times, two 119k-row inventories of the same directory
   (`lake_objects` vs `backup_archived_file`) never reconciled — the
   147-row gap has no owner and one drift direction is undetectable.
   Add a `virtues doctor` reconciliation check.
7. **Prefix-fence gaps**: `extracted_document_chunks` holds owner document
   text, is a first-class ontology, and is structurally unreachable by
   `sql_query` (unprefixed). Rename `data_document_chunk` + catalog entry
   — the one rename that unlocks capability. `credentials`/`box_secrets`
   stay unprefixed on purpose (the fence IS the security property).

## F. Suggested sequence

1. Fix A1–A4 (four small commits; A1 gets a wiring test in the
   `rules_reach_the_assembled_prompt` mold).
2. Add the two guard tests (catalog↔writer check; catalog↔registry
   reconciliation) — they freeze the phantom-column class before cleanup.
3. One cleanup migration: B tables + C columns/indexes + narrow the
   aspirational CHECKs. Sweep API structs/client types that serialized
   the dropped columns.
4. D decisions as they're made.
5. Convention fixes (E4, E5) ride along on tables already being touched —
   never as a standalone churn migration.
