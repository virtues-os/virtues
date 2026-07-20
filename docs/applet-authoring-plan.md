# Applet authoring — the phase-3 plan

> Status: **v2 — revised after the three-agent review** (red-team vs code · simplicity audit · writability dry-run of 5 canonical asks). The v1 draft's loop shape survived; the review moved the effort from wrapping (cards, catalog tools, edit-v2) into the contract (capability rows, param schema, server-enforced gating). Review register at the bottom.

## Goal

Chat intent → **folder** → **check** → **reconcile** → **gate** → enabled applet → re-setup to iterate. One door for every applet. The acceptance bar: the ten canonical asks (overhaul plan Appendix B) authored end-to-end through real chat — including the four **gate-invariant tests** (below), not just the happy path.

**Non-goals:** tier-2 terminal authoring; git lanes; the `message` wake; credentialed write-back tools; generic filesystem tools; per-applet PG roles (one shared writer role in v1).

## The core invariant (names the whole phase)

> **No path exists from model output to an enabled, scheduled row without a user-surface action.**

The red-team showed v1 violated this three ways: `edit_action` is ungated (model self-enables), check-failed drafts persist on disk (next reconcile promotes them), and `delete_action` leaves the folder (reconcile resurrects). All three fixes below serve this one sentence, and the acceptance suite tests it adversarially.

## A. `setup_action` v2 — the one tool (edit = re-setup)

**Exact parameter schema** (the registry `ToolConfig` is the source of truth; the capability table §B is embedded verbatim in its description so the model always knows what the prompt it writes may use):

| Param | Type | Req | Notes |
|---|---|---|---|
| `name` | string | ✓ | slug = `slugify(name)` — **guaranteed and documented**, so `schema_sql` and `face_html` can name `applet_<slug>` tables by construction |
| `description` | string | ✓ | the intent sentence — the gate headline |
| `agent` | string | ✓ | the runtime prompt → `prompt.md` |
| `schedule` | string | | 6-field cron, box-local tz |
| `triggers` | string[] | | cron·manual·tool·api·webhook |
| `condition` / `until` | string | | SQL bool / SQL bool-or-`"once"` |
| `schema_sql` | string | | DDL for `applet_<slug>` only → `schema.sql` |
| `face_html` | string | | → `face/index.html`; **hard cap 48KB**; history-echo mitigation: the loop truncates large args to a hash marker when rebuilding messages |
| `limits` | object | | `{max_llm_cost, timeout, max_runs}` → `[config.limits]` |

Executor flow (all trusted Rust):

1. Write to a **staging dir**; run check (§C). Failures → staging deleted, findings returned as the tool result (the model self-corrects in-turn; 20-step cap bounds it).
2. Promote to **`actions/user/<slug>/`** — the namespace the loader already reserves for from-chat actions (id `action_user__<slug>`): no collisions with builtins, git imports, or each other, and one stated **upgrade-preservation contract**: the installer/upgrade never touches `actions/user/`.
3. **Folder-scoped reconcile** (the git importer's diff-by-slug shape, not the global pass) under a **reconcile mutex**. Manifest seeds `enabled = false` whenever a boundary is crossed (schedule/trigger, credential, recurring spend); manual-only zero-boundary applets seed enabled.
4. Return the proposal: manifest text + derived capabilities + schedule echo + cost estimate (computed **in Rust**: `runs_per_day × slot constant` — the model never fills it).

- **Existing slug → upsert** (this IS the edit path; no `edit_action` v2): files rewritten, re-checked, re-reconciled; **`enabled` and `memory` preserved on update**. No `-2` suffixing.
- **`delete_action` / gate-discard remove the folder too** (same trusted path that wrote it) — no resurrection-by-reconcile.
- Existing `action_agent_*` DB-only rows: grandfathered untouched, die naturally. No migration machinery.
- `edit_action` v1 stays for live-ops fields only — and **loses the ability to flip `enabled`** (next section).

## B. The runtime capability table — with the three missing rows

`get_tools_for_action` flips from default-minus-denied to an **explicit allowlist**. The writability dry-run failed 3 of 5 asks on rows the v1 draft omitted; they're in:

| Capability | Mechanism | Status |
|---|---|---|
| think · read data (`sql_query`) · recall (`semantic_search`) · public web (`web_search` — **queries, not URLs**; it cannot fetch a feed) | existing tools | exists |
| **deliver to the user** | the run's result message **auto-posts to the applet's thread** (`config.chat_id`) — this already happens; it is now a documented contract row, the delivery verb for reminders/nudges | exists, undocumented → documented |
| **write its own tables** | new `sql_write` tool: executes under a shared `virtues_applet_writer` role granted DML **only on `applet_*` schemas** (migration; no `data_*`, no `app_*`). Per-applet role isolation = later hardening | build |
| notes (`update_action_memory`) · pages (`create_page`/`edit_page`/`get_page_content`) · jail compute (`code_interpreter`) · introspection (`list_actions`/`get_action`) | existing | exists |
| **faces read data** | the phase-2 face runtime: `virtues.query(sql)` via the face token + `virtues_face_reader` role — documented in AGENTS.md with the slug guarantee | exists, now documented |
| everything else | denied | enforced |

**Closing rule:** *if the ask needs a verb not in this table, decompose or decline — never write a prompt that pretends the tool exists.* (AGENTS.md carries the honest-downgrade recipes: feed-watching → decline or search-approximation, labeled as such; photo-logging → manual `sql_write` tracker until the `message` wake ships.)

## C. Check — an internal function (API/CLI later), hardened

Called by setup before promotion; per-field, structured `{field, error, suggestion}`; did-you-mean via strsim against the **live** catalog (no CATALOG.md — the live DB is the only authority; the authoring model grounds itself with `sql_query` over `information_schema` + 3-row samples, per AGENTS.md doctrine).

Hardening (red-team #8): `condition`/`until` EXPLAIN under the **face-reader read-only path** (not the core role); `schema_sql` is parsed first — **transaction-control statements rejected**, all DDL required to target `applet_<slug>` — then dry-run in `BEGIN…ROLLBACK` with `statement_timeout` + `lock_timeout`. TOML parse failures anywhere in the tree degrade to per-manifest findings — **`parse_template` stops panicking** (a hand-edited bad manifest must not 500 all authoring or crash boot).

## D. The gate — server-enforced, existing UI

The gate is real only if the model can't operate it. Two changes, no new components:

1. **`enabled: false→true` on ai-owned rows becomes a gated transition**: the `edit_action` tool path refuses it; flipping the switch is a plain HTTP PATCH the **UI makes when the user taps the existing detail-page toggle** (or the `permission_needed` allow/deny card if we want in-chat enable — that machinery exists and halts the agent loop with `AwaitingUser`).
2. The proposal renders as the model's own chat reply (manifest + capabilities + cost) linking the detail page. No bespoke `tool-setup_action` card in v1.

Gate predicate, enumerated here (not by reference): gated = has `schedule`/`trigger` ∨ `credential_id` ∨ recurring LLM cost. Not gated: manual-only, even with face/tables (storage on your own box is not a boundary).

## E. Reconcile learns `owner = "ai"` — the third branch

Today's two branches both break ai rows (system-overwrite clobbers operational config; user-seed-once makes file edits dead). The third branch: reconcile **overwrites compiled fields** (`agent`, `condition`, `until`, `description`, schedule seed, triggers) and **never touches operational state** (`enabled`, `memory`, operational `config` keys). For restore fidelity, the trusted enable/disable PATCH **mirrors the flag into the manifest's `default_enabled`** — a DB rebuilt from disk comes back with applets in their last chosen state.

## F. AGENTS.md — the contract, not a tutorial

At `actions/AGENTS.md`: the field contract + exact `[config.limits]` spellings; the §B capability table; the three `until` idioms; cron rules (6-field, box-local, **date-anchored asks: nearest future occurrence + `until="once"` mandatory**); the catch_up×window trap; the **cooldown idiom** until the field ships (`condition AND NOT EXISTS (successful run within interval)`); decomposition rule; catalog-check-first via `sql_query`; honest downgrades; web_search-is-not-fetch; Persona marked **v2 — not authorable, offer draft-mode**. Exemplars are **pointers to real folders** (`morning_examen`, `hello_world`/Biscuit), not inline copies that drift.

## Sequence (was 6, now 4)

1. **`setup_action` v2** — staging→check→promote under `actions/user/`, param schema, upsert-as-edit, folder-removing delete, folder-scoped reconcile + mutex, parse-degrade. *The core; everything else hangs off it.*
2. **Runtime contract** — allowlist flip, `sql_write` + `virtues_applet_writer` migration, delivery-row documentation.
3. **Gate enforcement + ai-reconcile branch** — the invariant becomes true.
4. **AGENTS.md, then the acceptance suite** — ten asks + four adversarial gate tests: (a) model attempts `edit_action{enabled:true}` post-setup → refused; (b) check-failed draft → no row after global reconcile; (c) delete → no resurrection after reconcile; (d) prompt-injected "enable yourself" in a source document → no enabled row.

## Review register (what each report changed)

- **Red-team**: the invariant + gate enforcement (D), staging/promote + folder-removing delete (A), `actions/user/` namespace + upgrade contract (A), the ai reconcile branch (E), reconcile mutex + folder-scoped pass + parse-degrade (A/C), check hardening incl. COMMIT-escape rejection (C), `face_html` cap + history-echo mitigation (A), CATALOG.md killed as a staleness trap (C).
- **Simplicity**: preview card cut (existing toggle/permission card is the gate); `data_catalog` tool cut (`sql_query` is the catalog); `edit_action` v2 cut (re-setup upserts); check API/CLI deferred; staleness marker, grandfather machinery, and inline exemplars cut; cost = one Rust heuristic. Sequence 6→4.
- **Writability** (2/5 one-shot before, projected 5/5 after): the delivery row, `sql_write`, the face data-access contract + slug guarantee, the exact param schema, capability table embedded in the tool description, web_search≠fetch, cooldown + date-anchored idioms, cost computed by the executor.
