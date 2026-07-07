# virtues-core dead-code audit

Date: 2026-07-02. Branch: `feat/iroh-pivot`.

Method: module-tree reachability + cross-reference grep across the **whole workspace**
(virtues-core is a `pub` lib, so the compiler's `dead_code` lint stays silent on unused
`pub` items — and sibling `actions/*` crates consume some of its public API, so
"unused inside virtues-core" ≠ dead). Everything below was checked against the entire repo.

## DONE — orphan files deleted

Two files were declared by no `mod` statement and referenced nowhere (not even compiled).
Deleted on this branch:

- `src/api/mcp_client.rs` (363 LOC) — "API functions for MCP server management"
- `src/mcp/client.rs` (337 LOC) — "MCP Client Manager"

Both were leftovers from a dropped "connect to *external* MCP servers" feature. The MCP
**server** (`src/mcp/server.rs`, `src/mcp/http.rs`) is live and unaffected.

---

## DONE — zero-risk removals (build verified clean)

Removed on this branch; `cargo check -p virtues --all-targets` passes:

- [x] `src/entity_resolution/places.rs` — unused import `chrono::Duration` (compiler-confirmed)
- [x] `src/api/box_status.rs` — `fn net` test helper (+ its now-orphaned `net_check` import), compiler-confirmed
- [x] `src/api/devices.rs` — `_value_ref` dummy (`Value` is genuinely used elsewhere, so the hack was pointless)
- [x] `src/setup/validation.rs` — whole module deleted (5 unused pub fns) + `pub mod` line
- [x] `src/storage/stream_writer.rs` — whole file deleted (retired `StreamWriter`) + `pub mod` line
- [x] `src/mcp/tools.rs` — whole file deleted; its only consumer was the already-deleted `mcp/client.rs` (the live `convert_rows_to_json` lives in `tools/sql_query.rs`) + `pub mod` line + stale doc ref in `types/timestamp.rs`

## TODO — intra-module dead code (NOT yet deleted; logged for review)

### Explicit `#[allow(dead_code)]` sites still present (kept — verify intent before touching)

- [ ] `src/cli/restore.rs:273` — `fn read_all(path)` — unused helper
- [ ] `src/api/pages.rs:410` — `struct RawEntitySearchResult` — unused
- [ ] `src/mcp/server.rs:32` — `pool` field held but never read
- [ ] `src/api/places.rs:48` — field `session_token`; `:98` — field `display_name` (unread)

### High-confidence unused `pub` items (zero refs anywhere in the workspace)

- [ ] `src/api/action_events.rs:108` — `subscribe_action_events` (handler never registered)
- [ ] `src/api/assistant_profile.rs:193` — `get_coding_model`
- [ ] `src/api/chat_usage.rs:21` — `ChatUsageRecord` (never instantiated)
- [ ] `src/api/compaction.rs:653` — `needs_compaction`
- [ ] `src/api/day_illustration.rs:45` — `run_illustration_job`
- [ ] `src/api/drive.rs:120` — `with_tier`; `:127` — `local_dev`; `:600` — `extract_lake_object_id`
- [ ] `src/api/metrics.rs:13` — `ActivityMetricsQuery`
- [ ] `src/api/namespaces.rs:105` — `parse_postgres_config`; `:111` — `parse_filesystem_config`
- [ ] `src/api/token_estimation.rs:149` — `estimate_verbatim_context`
- [ ] `src/mcp/tools.rs:16` — `convert_rows_to_json` (duplicate of the live one in `src/tools/sql_query.rs:232`)
- [ ] `src/tools/mod.rs` — `get_all_tool_definitions_for_llm` (leftover; mode-specific variants are used)

### Whole modules with no live public surface

- [ ] `src/setup/validation.rs` — all 5 pub fns unused (`test_database_connection`,
      `test_local_storage`, `display_error`, `display_success`, `display_info`).
      Declared `pub mod validation;` in `src/setup/mod.rs` but never used.
- [ ] `src/storage/stream_writer.rs` — `StreamWriter` + all methods unused. A comment in
      `src/server/mod.rs` ("StreamWriter is in-memory only now") confirms it was retired but
      not removed.

### Possibly dead / verify before touching

- [ ] `src/search/embedder.rs:61` — `trait Embedder` has one impl (`LocalEmbedder`) and is
      never used as `dyn Embedder`; could collapse the trait, but verify no generic bound relies on it.

---

## Verified NOT dead (false positives caught during audit)

These read as unused *within* virtues-core but are entry points called by sibling `actions/*` crates:

- `src/api/narrative_identity_gen.rs` `generate_narrative_identity_draft` → `actions/narrative_identity_draft`
- `src/dayline/sleep.rs` `resolve_sleep_events` → `actions/day_summary_eod`
- `src/search/indexer.rs` `run_embedding_job` (re-exported via `search::`) → `actions/embedding_index`
