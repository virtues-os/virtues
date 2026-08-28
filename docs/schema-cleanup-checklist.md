# Schema cleanup — the working checklist

*Companion to [schema-audit-2026-08-28.md](schema-audit-2026-08-28.md),
which holds the evidence. This is the do-list, ordered for incremental
work: fix → guard → rm → decide → later. Check items off as they land.
Every RM that touches a column needs the CLAUDE.md rename sweep (SQL
strings, `row.get`, catalog, API structs, client types).*

## 1 · FIX — real bugs, one commit each

- [ ] **F1** `entity_article_gen.rs:216,245` — `ref_count` → `seen_count`
      (person + place branches). Also `sql_query.rs:252,258,264` catalog
      entries (advertise `seen_count`, drop phantom `ref_count`) and the
      stale comment at `home.rs:165`. Add a wiring test in the
      `rules_reach_the_assembled_prompt` mold so article dossiers break
      loudly on the next rename.
- [ ] **F2** `applets/morning_examen/manifest.toml:36-42` — pre-rename
      column names (`start_time`/`end_time`/`timestamp`) → current names.
      Failing quietly since 2026-08-17.
- [ ] **F3** `dayline/context.rs:227` — `ORDER BY visit_duration_seconds
      DESC` (always NULL) → `ORDER BY occurred_at DESC`.
- [ ] **F4** `extracted_document_chunks` — add `occurred_at` sourced from
      the file's own date, repoint `ontologies.rs:1388,1401`. Until then
      every date-scoped document search filters on parse time.
- [ ] **F5** `app_drive_usage` — verify the write path (UPDATE vs upsert)
      against the never-seeded singleton; seed it or convert to upsert.
- [ ] **F6** `app_ai_calls.status` — write `'error'` on failed calls (the
      Usage grid currently shows "ok" for everything).
- [ ] **F7** `credentials.last_seen_at` — bump on successful use, or stop
      rendering it; today it says "never seen" for api-key/Plaid creds in
      daily use.
- [ ] **F8** `wiki_day_prose` fence mismatch — either add the view to
      `get_table_metadata()` + the pg_tables query, or remove the
      `JOIN wiki_day_prose` hint at `sql_query.rs:272-275`.
- [ ] **F9** Ghost selects: remove `act_id`/`chapter_id` from
      `api/wiki.rs:148-153,969-970`, the wire fields at
      `apps/web/src/lib/wiki/api.ts:93-94`, and the orphaned frontend
      types (`types/year.ts`, `acts`/`chapters` fields in
      `types/place.ts:115`, `types/organization.ts:86`).

## 2 · GUARD — tests that freeze the disease classes

- [ ] **G1** Extend `tools/check-dynamic-inserts.py` with the inverse
      check: every `sql_query.rs` `key_columns` entry must exist in the
      schema AND appear in some writer's column list. Retires the
      phantom-column class permanently.
- [ ] **G2** Reconciliation test between `registered_ontologies()` and
      `get_table_metadata()` — every divergence must be on an explicit
      allowlist with a reason (the `entities.rs:717` pattern, pointed at
      the second catalog).
- [ ] **G3** One-off: reconcile the 102 orphaned `search_embeddings` rows
      that have no `search_vectors` row (crash between the two writes).

## 3 · RM — dead tables (zero-risk, one commit each)

- [ ] **R1** `app_mcp_servers` + `app_mcp_tools` + the two *uncompiled*
      modules (`api/mcp_client.rs`, `mcp/client.rs` — never declared in
      any mod.rs) + their indexes and FK.
- [ ] **R2** `wiki_years` + trigger + UNIQUE index.
- [ ] **R3** `wiki_narrative_interview` + `api/narrative_interview.rs` +
      route (`server/mod.rs:296-300`) + client fns
      (`client.ts:383-414`, zero callers).
- [ ] **R4** `data_activity_listening` + descriptor
      (`ontologies.rs:842-867`) + `listening` LaneMeasure (`:325`) +
      dayline read (`context.rs:250-270`) + index.
- [ ] **R5** `app_applet_package` + the INSERT at
      `applet_git_import.rs:113` (or file a note if git-import packages
      get a reader someday — today: zero SELECTs).
- [ ] **R6** `app_auth_event_archive` — sweeper DELETEs instead of
      archiving; drop table + index. (90-day-old auth events with no
      query path are retention risk, not history.)

## 4 · RM — the one cleanup migration (columns, indexes, CHECKs)

Batch these into a single migration + its code sweeps. Grouped so each
group is reviewable; land groups as separate commits against the same
migration file before renaming `.sql.pending`.

- [ ] **R7** Unread indexes: `idx_lake_objects_kind`,
      `idx_lake_objects_stream`, `idx_lake_objects_replay`,
      `idx_backup_archived_file_increment`, `idx_storage_volume_state`,
      `idx_app_applet_runs_parent`, `idx_calendar_block_type`,
      `idx_wiki_days_dirty`, `idx_wiki_events_dirty`.
- [ ] **R8** `wiki_days`: drop `illustration`, `morning_baseline`,
      `battery_curve`, `segmented_at`, `dirty_at`. (`readiness_*` → D7.)
- [ ] **R9** `wiki_events`: drop `hrv_z`, `lof_raw`, `user_created`,
      `dirty_at`. Sweep the two reset paths (`cli/reindex.rs:118`,
      `cli/configure_inference.rs:136`) and the wire.
- [ ] **R10** `wiki_refs`: drop `confidence`, `resolved_by`, `metadata`
      (three dead columns × 100k+ rows).
- [ ] **R11** `app_chats`: drop `trace`, `action_instruction`.
- [ ] **R12** `app_applet_runs`: drop `parent_run_id`, `transform_stage`
      + the two always-null JSON keys in `client.ts:118-119`.
- [ ] **R13** `app_drive_usage`: drop `quota_bytes`, `data_lake_bytes`,
      `total_bytes`, `last_scan_at`, `last_scan_bytes`, `trash_bytes`,
      `trash_count`, `warning_80/90/100_sent`; delete
      `check_usage_warnings` (`api/drive.rs:414`, uncalled) and the
      `data_lake_bytes` UI segment (`DriveView.svelte:537,582`).
- [ ] **R14** `app_user_profile`: drop `crux`, `technology_vision`,
      `pain_point_primary`, `pain_point_secondary`, `excited_features`.
- [ ] **R15** `app_pages.date` + `get_reflections_for_date` +
      `/api/pages/reflections/:date` + JournalCard — the retired
      reflections feature, wired end-to-end, returning `[]` forever.
- [ ] **R16** Small leaves: `app_assistant_profile.embedding_model_id`,
      `app_auth_user.is_owner`, `app_ai_calls.chat_id`,
      `app_notebook_items.similarity`,
      `app_page_versions.content_preview`.
- [ ] **R17** `data_calendar_event`: drop `event_type`, `conference_url`,
      `conference_platform`, `recurrence_rule`, `timezone`, `block_type`
      — unless D7 decides to populate from Google (the API returns them).
      Remove the phantom entries from `sql_query.rs:101` either way.
- [ ] **R18** `data_activity_app_session`: drop `url`, `document_path`,
      `app_category` (+ catalog entry `sql_query.rs:139`, and the `url`
      mention in `morning_examen/manifest.toml:39`).
- [ ] **R19** `data_activity_web_browsing`: drop
      `visit_duration_seconds`, `scroll_depth_percent` (+ catalog
      `sql_query.rs:145`). Lands with F3.
- [ ] **R20** Scattered never-written data columns:
      `data_health_workout.route_geometry`,
      `data_communication_transcription.speaker_segments`,
      `data_financial_account.institution_id`,
      `data_content_bookmark.content_type`,
      `data_content_conversation.{model,tags,metadata}`,
      `data_financial_transaction.authorized_timestamp`,
      `data_audio_recording.audio_format` (constant 'm4a'). Catalog
      sweeps: `sql_query.rs:111,161,167`.
- [ ] **R21** Search: drop `search_embeddings.text_hash`,
      `search_index_meta.fingerprint`, `search_index_meta.built_at`.
      Sweep the NULL-resets in `cli/reindex.rs` /
      `cli/configure_inference.rs`.
- [ ] **R22** Infra: drop `lake_objects.source_id`,
      `backup_archived_file.{size_bytes,archived_at}`,
      `storage_volume.{capacity_bytes,free_bytes,probed_at,
      last_error_at,roles}` (+ the roles CHECK and the four
      `= ANY(roles)` scans), `credentials.scopes`.
- [ ] **R23** Narrow aspirational CHECKs to produced values:
      `lake_objects.kind` (drop `'drive'`), `content_encoding` (drop
      `'zstd'` — or the whole column: it is a constant),
      `storage_volume.kind` (drop `'internal'`,`'network'`), `state`
      (drop `'degraded'`), `credentials.status` (drop
      `'reauth_required'`,`'error'`), `app_user_profile
      .onboarding_status` (drop `'complete'`).
- [ ] **R24** 0008 leftovers outside the DB: fix dead path
      `applets/AUTHORING.md:139`, `"Biscuit"` in
      `applets/MANIFEST_SCHEMA.json:135`, delete/refresh the
      `hello_world` references in `agents/plan/display-plan.md` and
      `applet-authoring-plan.md`.
- [ ] **R25** Dead SET clause `id = EXCLUDED.id` at
      `extraction/mod.rs:272` (no-op PK self-assignment).

## 5 · DECIDE — product calls (each unblocks an RM or a build)

- [ ] **D1** Quota subsystem: wire `record_service_usage` into every
      metered egress, or delete `app_api_usage` + `app_usage_limits` +
      `check_limit`/`get_all_usage`/`Tier` (+ both float
      `estimated_cost_usd` columns). `app_ai_calls.cost_micros` already
      does the real accounting. *Leaning: delete.*
- [ ] **D2** `wiki_stories`: build the writer or cut the read path + UI.
- [ ] **D3** `app_erasure`: keep-as-reserved (add a code-visible "not
      implemented" marker) or cut until the sweeper is real.
- [ ] **D4** Gmail: 30-day cold start is by design, no backfill path
      exists. Want backfill?
- [ ] **D5** `data_health_workout` = 0 rows despite two working writers —
      check real `stream_ios_healthkit` payloads on the box; collector
      never emits workouts, or the type match fails.
- [ ] **D6** Email recipient columns (`to_names`,`cc_emails`,
      `bcc_emails`): keep only if recipient-side entity resolution is
      planned (today sender-only). Else RM.
- [ ] **D7** Readers-with-no-writer: `data_calendar_event.is_sacred`
      (read at home.rs:116/wiki.rs:2630, always false),
      `wiki_days.readiness_score`/`readiness_details` (rendered in
      DaylineChart/DayPage, never written), `wiki_days.snapshot`
      (wire-only). Build the writers or drop reader+column together.

## 6 · LATER — real, not now (ride along or wait for cause)

- [ ] **L1** `data_communication_message`: promote
      `metadata->>'is_from_me'` (text 'true'/'false' × 171k rows,
      load-bearing in 3 lane measures) to a typed `is_from_me boolean`,
      matching `email.direction`.
- [ ] **L2** Audio triad dedup: `data_communication_transcription`
      carries 4 columns copied from the joined recording;
      `data_audio_session.content` is a third copy of the words. Collapse
      recording toward a pure blob pointer. Also: census counts
      recordings (68.6% silence) — count conversations instead.
- [ ] **L3** `extracted_document_chunks` → `data_document_chunk` +
      catalog entry — the one rename that unlocks capability (owner's
      documents become SQL-reachable). Full rename sweep.
- [ ] **L4** `lake_objects.min_timestamp`/`max_timestamp` →
      `min_occurred_at`/`max_occurred_at`.
- [ ] **L5** Money: `_cents` suffix on the 8 bigint-cents columns
      (financial account/transaction/asset/liability), state the unit in
      the asset/liability catalog descriptions (`sql_query.rs:121,127`)
      — today an agent reports holdings 100× too large. Float
      `estimated_cost_usd` dies with D1.
- [ ] **L6** Boolean renames (`enabled`→`is_enabled` etc., `active` vs
      `is_active`) and instant renames (`sampled_at`, `completed_at`,
      `last_modified_time`, `sleep_start`/`sleep_end` from 0005) — ride
      along on tables already being touched, never standalone churn.
- [ ] **L7** Singleton unification: four bespoke patterns → one (the
      named `singleton boolean` CHECK). Convert on touch; fix
      `app_auth_user`'s missing CHECK and 0004's `id boolean` when those
      tables next change.
- [ ] **L8** `virtues doctor` reconciliation: `lake_objects` vs
      `backup_archived_file` (two 119k-row inventories of one directory,
      147-row gap unowned — one drift direction currently undetectable).
- [ ] **L9** `wiki_events.embedding bytea` — a private second embedding
      store outside search_vectors (no HNSW, no dim guard). Fold into
      the search infra when the dayline is next touched.
- [ ] **L10** `app_chats.message_count` — hand-maintained counter in 3
      places; drop it or add a doctor drift check.
- [ ] **L11** Duplicate-truth timestamp pairs
      (`app_applet_runs.started_at`+`created_at`,
      `app_device.paired_at`+`created_at`) — drop one of each on touch.
- [ ] **L12** Legacy model-id pair (`default_model_id`/
      `background_model_id` vs `chat_model_id`/`lite_model_id`) resolved
      by fallback chains in 3 places — collapse when Models settings are
      next touched.
- [ ] **L13** HealthKit double-storage (`metadata.healthkit_raw` beside
      the typed columns) — decide a retention policy for raw payloads.
- [ ] **L14** Fossil constraint/sequence names
      (`*_played_at_not_null`, `app_space_items_id_seq`, stale index
      names in the dump) — cosmetic; only worth it inside a future
      squash.
