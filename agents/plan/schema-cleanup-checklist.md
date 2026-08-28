# Schema cleanup — the working checklist

*Companion to [schema-audit-2026-08-28.md](../record/schema-audit-2026-08-28.md),
which holds the evidence. This is the do-list, ordered for incremental
work: fix → guard → rm → decide → later. Check items off as they land.
Every RM that touches a column needs the CLAUDE.md rename sweep (SQL
strings, `row.get`, catalog, API structs, client types).*

## 1 · FIX — real bugs, one commit each

- [x] **F1** ✅ `f6825723` — `entity_article_gen.rs:216,245` — `ref_count` → `seen_count`
      (person + place branches). Also `sql_query.rs:252,258,264` catalog
      entries (advertise `seen_count`, drop phantom `ref_count`) and the
      stale comment at `home.rs:165`. Add a wiring test in the
      `rules_reach_the_assembled_prompt` mold so article dossiers break
      loudly on the next rename.
- [x] **F2** ✅ `ff9d80ba` — `applets/morning_examen/manifest.toml:36-42` — pre-rename
      column names (`start_time`/`end_time`/`timestamp`) → current names.
      Failing quietly since 2026-08-17.
- [x] **F3** ✅ `5a2583b1` — `dayline/context.rs:227` — `ORDER BY visit_duration_seconds
      DESC` (always NULL) → `ORDER BY occurred_at DESC`.
- [x] **F4** ✅ `40f8440a` (migration 0009) — `extracted_document_chunks` — add `occurred_at` sourced from
      the file's own date, repoint `ontologies.rs:1388,1401`. Until then
      every date-scoped document search filters on parse time.
- [x] **F5** ✅ `31f5d08c` (add path was already upsert; the reconcile was the remaining UPDATE-only no-op) — `app_drive_usage` — verify the write path (UPDATE vs upsert)
      against the never-seeded singleton; seed it or convert to upsert.
- [x] **F6** ✅ `a3e7bcd0` — resolved as REMOVAL, not error-writing: failed calls never produce rows at all (recording comes from usage data), so a status column on success-only rows can only ever say ok. Swept from SELECT/struct/wire; column drop rides the cleanup migration.
- [x] **F7** ✅ `36b2da53` — bumped in read_credential_secrets (the one decrypt gate), 60s-throttled, best-effort. — bump on successful use, or stop
      rendering it; today it says "never seen" for api-key/Plaid creds in
      daily use.
- [x] **F8** ✅ `65816c4c` — the view got a catalog entry and list_tables unions pg_views. — either add the view to
      `get_table_metadata()` + the pg_tables query, or remove the
      `JOIN wiki_day_prose` hint at `sql_query.rs:272-275`.
- [x] **F9** ✅ `6adb371e` (backend + wire; the place/org `narrativeContext` doc-comments were not ghosts and stay) — remove `act_id`/`chapter_id` from
      `api/wiki.rs:148-153,969-970`, the wire fields at
      `apps/web/src/lib/wiki/api.ts:93-94`, and the orphaned frontend
      types (`types/year.ts`, `acts`/`chapters` fields in
      `types/place.ts:115`, `types/organization.ts:86`).

## 2 · GUARD — tests that freeze the disease classes

*(Incidental find while landing F4, fixed in `9f9538f3`: the docs-split
commit had edited comments inside FOUR already-applied migrations —
0005 + three atlas files — which would have made every staging.65–.71
box refuse to boot on the next prerelease and broken the next atlas
deploy. Files restored byte-for-byte. Candidate **G4**: a CI check that
migrations/ files never change once they exist on origin/staging.)*

- [x] **G1** ✅ `49d8d8d6` — done, and it found two blind spots in the
      ORIGINAL check too (`build_batch_upsert_query` was never validated
      at all; backslash-continued INSERT strings were skipped). Catalog
      swept of 8 audited phantoms + `content_summary`; wired into CI
      after the migrate step. Green: 19 writer lists + 37 catalog
      entries. Extend `tools/check-dynamic-inserts.py` with the inverse
      check: every `sql_query.rs` `key_columns` entry must exist in the
      schema AND appear in some writer's column list. Retires the
      phantom-column class permanently. *(Verifier addition: the scan
      must include `virtues-core/seeds/*.sql` INSERT column lists — raw
      SQL seeds are a writer class the current check cannot see, and
      they hid five "never written" columns from the audit.)*
- [x] **G2** ✅ `39afd005` — `registry_and_sql_catalog_agree_or_divergence_is_named`,
      with stale-allowlist detection (an explanation cannot outlive its
      divergence). Passed first try, confirming the audit's inventory.
      Reconciliation test between `registered_ontologies()` and
      `get_table_metadata()` — every divergence must be on an explicit
      allowlist with a reason (the `entities.rs:717` pattern, pointed at
      the second catalog).
- [x] **G3** ✅ **FALSE ALARM, resolved by diagnosis**: all 102
      vectorless rows are `model='skip'` — empty records, vectorless BY
      DESIGN (the indexer writes a hash-only row for empty text so it
      doesn't respin on it). Zero crash-window orphans exist; the
      embed/vector writes turn out to share one transaction. No code
      change; recorded so nobody "fixes" it later.
- [x] **G4** ✅ `9d8d6a67` — applied migrations are append-only in CI
      (PR-only step; allows new files + consumed `.sql.pending`; flags
      any M/D on existing `.sql` in core or atlas chains). Carries a
      temporary allowlist for the four files 9f9538f3 restored — DELETE
      it once this wave merges to staging.

## 3 · RM — dead tables — **LANDED 2026-08-28, migration 0010** (`3b265dc8`)

All nine tables dropped in one migration, code sweeps first, full lib
suite green (539 passed) and the G1 guard green after.

*Every item below survived a fresh refutation attempt. Caveats are
mandatory co-deletions, not suggestions.*

- [x] **R1** ✅ `31f9bffa` — `app_mcp_servers` + `app_mcp_tools` + the two
      *uncompiled* modules (`api/mcp_client.rs`, `mcp/client.rs` —
      verified three ways: no mod declaration, no `#[path]`, no external
      callers of any pub item) + indexes + FK. Co-delete two doc-comment
      mentions: `crates/virtues-registry/src/lib.rs:23`, `tools.rs:6`.
- ~~**R2** `wiki_years`~~ — **KEPT** (Adam, 2026-08-28): the table stays.
- [x] **R3** ✅ `9904c760` — `wiki_narrative_interview` + `api/narrative_interview.rs`
      + route (`server/mod.rs:296-300`) + client fns (`client.ts:383-414`,
      zero callers). **Verifier additions:** must also delete the live
      catalog entry `tools/sql_query.rs:240-245` (else the agent is told
      to query a dropped table) and fix the doc comment
      `agent/prompt.rs:56`. Drafter is unaffected (reads the chat
      transcript, not this table).
- [x] **R4** ✅ `33b6b30e` — `data_activity_listening` — **descriptor and table must
      drop in the SAME change**: stream_health/lifeline/census/wiki all
      `format!`-build SQL from the registry, and stream_health's single
      UNION takes every stream down if the table is gone but the
      descriptor lives. Co-deletions beyond the audit's list:
      `crates/virtues-registry/src/tools.rs:542` (prompt-visible table
      list), `apps/web/src/lib/wiki/ontology.ts:22`, LaneMeasure
      `ontologies.rs:325`, `dayline/context.rs:250-270`, index +
      trigger + named constraint in 0001.
- [x] **R5** ✅ `2fde58a2` — `app_applet_package` + the INSERT at
      `applet_git_import.rs:113`. Note: forecloses the "is there a newer
      version" check for git-imported applets (the column's stated
      purpose) — acceptable, feature never built.
- [x] **R6** ✅ `2fde58a2` — `app_auth_event_archive` — sweeper's CTE move-then-delete
      (`sweeper.rs:112`) becomes a plain DELETE; drop table + index +
      the doc comment at `sweeper.rs:17-18`.
- [x] **R26** ✅ `3b265dc8` — *(corrected by verification)* Quota subsystem:
      `app_api_usage` + `app_usage_limits` + `api/usage.rs` fns + boot
      seed (`server/mod.rs:56`) + re-export block (`api/mod.rs:236-239`)
      + **six** call sites: 3× `record_usage`
      (`server/api.rs:1305,1351,1672`) and 3× `check_limit`
      (`server/api.rs:1284,1331,1646` — Google Places ×2, Parallel
      search). **Corrections:** only `app_api_usage.estimated_cost_usd`
      dies — `app_chat_usage.estimated_cost_usd` is NOT quota, it's the
      cost figure rendered in the chat context panel
      (`ContextViewPanel.svelte:235`); keep it (float→cents is L5).
      Removing `check_limit` removes the *intended* guardrail on two
      paid APIs — in practice it never fired (the counter was always 0),
      but note it: if real caps are ever wanted, they come back as a
      simple per-call budget, not this subsystem.
- [x] **R27** ✅ `3b265dc8` (migration 0010) — `app_erasure` — cleanest item verified: zero references
      outside migration 0003; outbound FK only; indexes + CHECK drop
      with the table.

## 4 · RM — the column tier — **LANDED 2026-08-28, migration 0011** (`359739e0`)

Nine indexes, ~55 columns across 20 tables, four CHECKs narrowed, sweeps
first; suite 538 green, catalog guard green, demo seed dry-run green.
One more seed writer surfaced post-drop (wiki_days.morning_baseline) —
and the seed turned out to be the writer behind the readiness_* mystery
(L16 half-resolved: seed-writes-it, UI-renders-it, production never
writes it).

One migration + its code sweeps. **Ordering rules the verification
established (load-bearing, read before executing):**

1. **Code sweeps land BEFORE the migration.** Removing a struct field,
   SELECT-list entry, or INSERT column is compatible with the old
   schema; the reverse order breaks at runtime (`SELECT *` + `FromRow`
   / non-optional `try_get` decode failures that sqlx cannot catch at
   compile time — the R12/R14/R16 class).
2. **`DROP INDEX IF EXISTS`** for the four indexes whose columns other
   items drop (`parent_run_id`, `block_type`, both `dirty_at`) — the
   column drop cascades the index away first.
3. **The demo seed is a writer**: `virtues-core/seeds/demo_day.sql` runs
   via `raw_sql` and one missing column aborts the ENTIRE demo seed.
   Every column it names must be co-edited there.
4. **Grep hazards** (same name, different live thing):
   `wiki_notes.resolved_by`, `wiki_events.confidence`, the *computed*
   `DriveUsage.quota_bytes`/`total_bytes` struct fields (load-bearing
   in the storage bar UI), and `free_bytes` as a common Rust fn name.

- [x] **R7** ✅ `359739e0` — Unread indexes: `idx_lake_objects_kind`,
      `idx_lake_objects_stream`, `idx_lake_objects_replay`,
      `idx_backup_archived_file_increment` (PK already covers its only
      predicate), `idx_storage_volume_state`,
      `idx_app_applet_runs_parent`, `idx_calendar_block_type`,
      `idx_wiki_days_dirty`, `idx_wiki_events_dirty`. Do NOT touch the
      partial UNIQUE on `lake_objects.sha256` — load-bearing dedup.
- [x] **R8** ✅ `b5d396a6`+`359739e0` *(amended)* `wiki_days`: `illustration` and `dirty_at`
      are clean drops; `segmented_at` needs its one writer swept
      (`day_summary.rs:360`); `morning_baseline` + `battery_curve` are
      always-NULL but **actively SELECTed** — co-sweep six read sites
      first (`api/wiki.rs:908,935,971-972,1235,154-155` +
      `apps/web/src/lib/wiki/api.ts:95-96`) or every day query breaks.
      The `wiki_day_prose` view reads only `d.id`/`d.date` — clear.
      (`readiness_*` → L16.)
- [x] **R9** ✅ `b5d396a6`+`359739e0` (lof_raw/user_created/dirty_at dropped; **hrv_z KEPT — Adam's call still open**) *(amended by refutation)* `wiki_events`: `lof_raw` and
      `dirty_at` are clean drops. **`hrv_z` is never written but
      rendered end-to-end** (`api/wiki.rs:1510,1599,1443,1721,1790` →
      `converters.ts:262` → `types/day.ts:49`) and autonomic scoring
      names it as intended-future ("would use -hrv_z"): either drop
      with the 7-site sweep or leave until scoring writes it — Adam's
      call. **`user_created` IS named in live SQL** (carry-over INSERT
      at `dayline/sleep.rs:208,211`) though never true: drop = sweep
      that INSERT + 5 read sites + `converters.ts:268` +
      `types/day.ts:59`. Sweep the two reset paths either way
      (`cli/reindex.rs:118`, `cli/configure_inference.rs:136`).
- [x] **R10** ✅ `359739e0` — `wiki_refs`: drop `confidence`, `resolved_by`,
      `metadata` — verified against all 96 references: no writer, no
      reader, no `SELECT *` on the table. Mind grep hazard #4.
- [x] **R11** ✅ `359739e0` (verified zero non-null rows on the box first) — `app_chats`: drop `trace`, `action_instruction` —
      every query uses explicit column lists. Caveat: `app_chats` is a
      registered ontology, so both currently appear in the record
      viewer via `to_jsonb`; check for non-null rows before dropping
      (user-visible data would disappear).
- [x] **R12** ✅ `dfc2355d`+`359739e0` ⚠️ *(refuted as written; droppable with ordering)*
      `app_applet_runs.parent_run_id` + `transform_stage` are never
      written but **actively read by a compiled mapper off
      `SELECT *`/`RETURNING *`** (`scheduler/applets.rs:1093-1094`,
      non-optional `?`) — a bare drop breaks run creation/detail/list
      at runtime. Sweep first: `applets.rs:113-114` (struct),
      `:1093-1094` (mapper), `client.ts:118-119`; then drop (self-FK +
      index cascade). Also sweep the stale comment
      `api/metrics.rs:224-226`.
- [x] **R13** ✅ `dfc2355d`+`359739e0` ⚠️ *(refuted in part)* `app_drive_usage`:
      `quota_bytes` + `data_lake_bytes` are clean (plus the
      `data_lake_bytes` UI segment, `DriveView.svelte:537,582` — NOT
      the computed struct fields of the same names, grep hazard #4).
      `total_bytes` (4 write sites: `drive.rs:362,366,391,1812`),
      `trash_bytes`/`trash_count` (3: `:1090,1332,1374`),
      `last_scan_at`/`last_scan_bytes` (1: `:1817-1818`) — sweep
      writes, then drop. **`warning_80/90/100_sent` are LIVE-read by a
      routed endpoint** (`/api/drive/warnings`: `server/mod.rs:751` →
      `api.rs:2956` → `check_usage_warnings`); no web caller, so
      delete endpoint + route + re-export (`api/mod.rs:91`) first,
      then the columns.
- [x] **R14** ✅ `dfc2355d`+`359739e0` — `app_user_profile` discovery fields — confirmed unread,
      but `profile.rs:49` is `SELECT *` into `FromRow`: delete
      `storage/models.rs:72-76` + the `profile.rs:38-42,89-93,151-163`
      request/apply blocks FIRST, then the migration.
- [x] **R15** ✅ `dfc2355d`+`359739e0` (the four dated rows were three empties + the word "dyad"; deleted) ⚠️ *(refuted as stated; still droppable)* `app_pages.date`
      has a live reader beyond the dead reflections route:
      `list_pages` filters `WHERE date IS NULL` (`pages.rs:221,230`) to
      keep old day-reflections out of the Pages list. Drop = same
      migration must `DELETE FROM app_pages WHERE date IS NOT NULL`
      (verify rows are disposable first — on the box these are
      pre-2026-08-03 auto-reflections). Sweep: 5 SELECT/RETURNING lists
      (`pages.rs:228,253,375,440,493`), `Page`/`PageSummary` structs
      (`:48,:62`), the filter, the test
      (`wiki_articles.rs:576-583`), route + handler + client fn +
      JournalCard mount (`DayPage.svelte:33,609`).
- [x] **R16** ✅ `dfc2355d`+`359739e0` *(amended)* `app_auth_user.is_owner` ✅ clean.
      `app_assistant_profile.embedding_model_id` — delete
      `storage/models.rs:98` first (`SELECT *`/`RETURNING *` decode).
      `app_ai_calls.chat_id` — written with real values; sweep struct
      field `ai_calls.rs:33` + 4 construction sites (`chat.rs:1824`,
      `applet_runner.rs:165`, `virtues_api/client.rs:565,577`,
      `executor.rs:83`). `app_notebook_items.similarity` — sweep the
      magnet INSERT (`magnet.rs:390`).
      ~~`app_page_versions.content_preview`~~ — **STRUCK: fully live**
      (written, selected, consumed by the frontend versions panel,
      `yjs/versions.ts:19,56`). The audit was wrong.
- [x] **R17** ✅ `1fa4284e`+`359739e0` ⚠️ *(refuted in part — the demo seed writes 4 of 6)*
      `data_calendar_event`: `recurrence_rule` ✅ clean. `event_type`,
      `conference_url`/`conference_platform`, `timezone` are written by
      `virtues-core/seeds/demo_day.sql` (5/3/5 INSERTs) — co-edit the
      seed (ordering rule #3). `block_type` — also swept from
      `OntologyDataTable.svelte:116` (badge switch); its partial index
      is in R7 (ordering rule #2). Catalog sweep `sql_query.rs:101`.
- [x] **R18** ✅ `1fa4284e`+`359739e0` ⚠️ `data_activity_app_session`: `document_path` ✅ clean;
      `url` + `app_category` are written by `demo_day.sql`
      (`:732-790`) — co-edit the seed, plus the `url` mention in
      `morning_examen/manifest.toml:39` (lands with F2).
- [x] **R19** ✅ `5a2583b1`+`1fa4284e`+`359739e0` ⚠️ `data_activity_web_browsing`: `scroll_depth_percent` ✅
      clean. **`visit_duration_seconds` has a second live reader**: the
      Dot Cloud face's `EVENTS_SQL`
      (`applets/dot_cloud/face/index.html:158`) — one UNION across nine
      tables, so a bare drop blacks out the whole default face. Co-edit
      the face SQL (it already `COALESCE(...,60)`s — remove the term),
      then drop. Catalog sweep `sql_query.rs:145`. Lands with F3.
- [x] **R20** ✅ `1fa4284e`+`359739e0` *(amended — 3 of 8 struck)* Confirmed clean drops:
      `data_health_workout.route_geometry`,
      `data_financial_account.institution_id`,
      `data_content_bookmark.content_type`,
      `data_content_conversation.{model,tags,metadata}`.
      `data_communication_transcription.speaker_segments` — droppable
      with a `demo_day.sql:933` co-edit.
      ~~`authorized_timestamp`~~ — **STRUCK: live Plaid writer**
      (insert + update paths + seed); it's real data, rename to
      `authorized_at` in L5 instead.
      ~~`audio_format`~~ — **STRUCK: actively read** — the
      transcription drain selects it into a non-Option field and picks
      the Gemini upload MIME type from it
      (`transcription_resolution/transform.rs:176,383,674`); dropping
      breaks transcription for every recording.
      Catalog sweeps: `sql_query.rs:111,161,167`.
- [x] **R21** ✅ `3280d2e3`+`359739e0` — Search: drop `search_embeddings.text_hash` (sweep 3
      INSERT lists: `indexer.rs:442,562`, `wiki_articles.rs:817`),
      `search_index_meta.fingerprint` (the live fingerprint is
      env-based, never this column), `search_index_meta.built_at`.
      Sweep the NULL-resets in `cli/reindex.rs` /
      `cli/configure_inference.rs`. The derive-id trigger doesn't touch
      `text_hash` — clear.
- [x] **R22** ✅ `3280d2e3`+`359739e0` (bonus: record_probe had NO callers — deleted whole) *(amended — source_id struck)*
      ~~`lake_objects.source_id`~~ — **STRUCK: carries real
      provenance** — for all nine cloud-sync applets it's the applet
      ACTION, not the provider (`storage/lake.rs:339-357`), and it's
      baked into storage-key paths. The audit's "always = provider"
      premise was wrong.
      Confirmed: `backup_archived_file.{size_bytes,archived_at}`
      (sweep the INSERT `cli/backup_volume.rs:216-218` + the
      `UNNEST($4)` plumbing at `:764-767`),
      `storage_volume.{capacity_bytes,free_bytes,probed_at,
      last_error_at}` (mind `free_bytes` the fn name — hazard #4),
      `storage_volume.roles` + CHECK + the four `= ANY(roles)` scans +
      struct field (`volumes.rs:28`) + `serves_backup()` (`:49`) +
      SELECT list (`:176`) + the unit test (`:292-306`),
      `credentials.scopes` (+ the never-constructed field
      `credentials/types.rs:71`).
- [x] **R23** ✅ `359739e0` *(amended — one value saved)* Narrow CHECKs:
      `lake_objects.kind` drop `'drive'`; `content_encoding` — drop the
      whole column (provably constant `'none'`); `storage_volume.kind`
      drop `'internal'`/`'network'`; `state` drop `'degraded'`;
      `app_user_profile.onboarding_status` drop `'complete'` (note: the
      profile API accepts client-supplied strings into this column —
      narrowing turns a bad value into a 500, which is correct but new).
      ~~`credentials.status`~~ — **KEEP both `'reauth_required'`
      (PRODUCED at `auth/refresh.rs:137-144` on provider-rejected
      refresh — narrowing would turn every failed refresh into a
      constraint violation) and `'error'`** (parsed on read, one line
      from produced).
- [x] **R24** ✅ `359739e0` (display-plan.md skipped — another agent's in-flight edit; its two lines remain) — 0008 leftovers: `applets/AUTHORING.md:139` dead path,
      `"Biscuit"` in `applets/MANIFEST_SCHEMA.json:135`, `hello_world`
      refs in `agents/plan/display-plan.md:46,119` +
      `applet-authoring-plan.md:81` + (verifier addition)
      `agents/record/applets-surface-audit.md:73`.
- [x] **R25** ✅ `359739e0` (legacy-id count verified 0 first) — Dead SET clause `id = EXCLUDED.id` at
      `extraction/mod.rs:272` — verified pure function of the conflict
      target. Run the one-line legacy-id sanity count against the live
      DB first (if any pre-formula rows exist, the clause is silently
      repairing them).

## 5 · DECIDE — resolved 2026-08-28

- **D1** Quota subsystem → **delete** (now R26).
- **D2** `wiki_stories` → **keep as-is**; revisit later.
- **D3** `app_erasure` → **rm** (now R27).
- **D4** Gmail backfill → confirmed: none exists (30-day cold start +
  cursor, `google_gmail_sync/main.rs:44`). Backfill is wanted → L15.
- **D5** `data_health_workout` = 0 → **expected, not a bug**: HealthKit
  records a workout only when one is started/confirmed (watch
  auto-detect still requires confirmation; iPhone-only never records
  workouts). Table stays; Strava also writes it when connected.
- **D6** Email recipient columns → **keep**; recipient-side resolution
  is planned.
- **D7** Readers-with-no-writer trio → parked to L16.

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
      — today an agent reports holdings 100× too large. Also rename
      `authorized_timestamp` → `authorized_at` (live Plaid data, struck
      from R20). *(Corrected: only `app_api_usage.estimated_cost_usd`
      dies with R26 — `app_chat_usage`'s is user-visible chat cost and
      converts float→`cost_micros` here instead.)*
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
- [ ] **L15** *(from D4)* Gmail backfill: list historical message ids,
      batched fetch through the existing transform, resumable cursor.
      A feature, not cleanup.
- [ ] **L16** *(from D7)* Readers-with-no-writer trio:
      `data_calendar_event.is_sacred` (always false),
      `wiki_days.readiness_score`/`readiness_details` (UI renders; the DEMO
      SEED writes them — production never does), `wiki_days.snapshot` (wire-only). Decide
      writer-or-drop when those surfaces are next touched.
