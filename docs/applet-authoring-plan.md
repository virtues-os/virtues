# Applet authoring — the phase-3 plan

> Status: **DRAFT — under review.** Child of [applets-overhaul-plan.md](./applets-overhaul-plan.md) (phases 0–2 built). This is the loop that makes "say it in chat, get an applet" real. Written against the surveyed machinery: agent loop (`agent/mod.rs`, streaming, max 20 steps), tool registry+executor (registry `ToolConfig` + executor match arm), `setup_action`/`edit_action`/`run_action`, `POST /api/admin/reconcile`, git import, model slots, and the ChatView special-card pattern.

## Goal

Chat intent → **folder** (manifest + prompt + optional face/schema) → **check** → **reconcile** → **preview gate** → enabled applet → **reply-driven iteration**. One door for every applet; chat-authored applets become first-class folder citizens (diffable, portable, git-lane-ready).

**Non-goals (phase 3):** tier-2 terminal/Claude-Code authoring; git lanes; the `message` wake; credentialed write-back tools (gmail-archive etc. — the *grant shape* is designed here, the tools ship with phase 4); generic filesystem tools.

## The keystone simplification: no file tools

The survey confirmed the chat agent has **zero filesystem tools** — and the authoring loop doesn't need them. The folder is written by the **`setup_action` executor in Rust from structured params**. The model fills fields (`name`, `description`, `agent`, `schedule`, `condition`, `until`, `face_html`, `schema_sql`); trusted code writes `manifest.toml`, `prompt.md`, `face/index.html`, `schema.sql`. This keeps scoping trivial (no path jail to build), keeps the artifact well-formed by construction, and stays true to "fill flat fields you already speak." Generic scoped file tools arrive only with the power tier.

## A. `setup_action` v2 — materialize, don't INSERT

Rework the executor (`tools/action_setup.rs`) from direct-SQL to:

1. Slugify `name` → folder `actions/<slug>/` (collision → `-2` suffix). Write `manifest.toml` (owner=`ai`, `enabled` seed **false**, `config.chat_id` = thread route) + `prompt.md` (referenced as `agent = "prompt.md"`) + optional `face/index.html`, `schema.sql`.
2. Run **check** (§C). Errors → return them as the tool result (the model self-corrects in the same turn; this is the loop).
3. Reconcile (existing `reload_catalog` + `reconcile_templates` + supervisor reload).
4. Return a **proposal** result: the manifest text, derived capabilities, estimated cost/day.

- **Id becomes folder-derived** (`action_<slug>`), replacing `action_agent_<chat_id>_<slug>`. Existing rows: grandfathered as-is (they keep working); the first edit materializes a folder with `id_prefix` pinned to the old id so run history FKs survive. No bulk migration.
- **Owner=`ai` reconcile semantics** = same as `user` (seed-once; SQL owns live state after). The folder is still rewritten by *edit* operations (§E) — reconcile just doesn't fight the UI.
- `until`, `schedule`, `condition`, `triggers`, `[config.limits]` params flow straight through (all exist since phase 1).

## B. The agent-runtime capability table — the missing contract

What a declarative (owner=`ai`) applet run may do. This becomes (a) an **explicit allowlist** replacing today's default-minus-denied in `get_tools_for_action`, (b) the centerpiece table of AGENTS.md, (c) the source of derived capabilities at the preview gate.

| Capability | Tool | Status |
|---|---|---|
| think / plan | `think` | exists |
| read the box's data | `sql_query` | exists (hardening to the face-reader pattern: phase-3 stretch) |
| semantic recall | `semantic_search` | exists |
| search the public web | `web_search` | exists |
| write its own notes | `update_action_memory` | exists |
| write durable output | `create_page`, `edit_page`, `get_page_content` | exists (pages = the output surface until threads) |
| compute in the jail | `code_interpreter` | exists (systemd-run, no network) |
| introspect applets (read) | `list_actions`, `get_action` | exists |
| **denied** | `setup_action`, `edit_action`, `delete_action`, `run_action`, `dispatch_subagents`, memory/profile/name tools, `generate_image` (cost), everything else | enforced in `get_tools_for_action` |

**Closing rule (AGENTS.md doctrine):** *if the ask needs a verb not on this list, decompose it or decline — never write a prompt that pretends the tool exists.* Future rows land as scoped tools with capability grants: `http_fetch` (per-applet domain allowlist in config), credentialed source verbs (gmail-archive — grant = the credential boundary prompt).

## C. `reconcile --check` — the compiler

A dry-run validation pass, exposed three ways: `POST /api/admin/reconcile?check=true`, CLI `virtues applet check [name]`, and **called internally by setup/edit before materializing sticks**. Returns structured findings:

```json
[{ "applet": "workout_nudge", "field": "condition", "error": "column \"hr\" does not exist",
   "suggestion": "data_health_heart_rate has: bpm, recorded_at, …" }]
```

Validators (per-field, all cheap):
- TOML parse + unknown-key warnings; name/slug/id collisions.
- `schedule`: cron parse (existing 5/6-field validation) + humanized echo in the result.
- `condition` / `until`: `EXPLAIN` under the hardened read-only path (reuses `eval_condition`'s transaction shape); `until` also accepts the literal `once`.
- `schema.sql`: `BEGIN … ROLLBACK` apply (PG transactional DDL).
- `command`: binary exists (power tier only); `triggers`: enum check; `[config.limits]`: known keys + types.
- **Did-you-mean**: on unknown-relation/column errors, strsim against the live catalog → suggestion string. This is the LSP; error text is the UX.

## D. The preview gate — a chat card

New `tool-setup_action` arm in ChatView (the `CodeInterpreterCard` pattern): renders the **manifest prettily** (the artifact IS the preview), the **intent sentence** as the headline, **derived capabilities** (from filled fields: reads health data · writes its own table · runs daily at 6am · ~$0.04/day), and **Enable / Discard** buttons. Enable → `edit_action {enabled:true}` (the applet was materialized disabled — the gate is real, not decorative). Zero-boundary applets (manual-only, no credential, no schedule, no spend) may auto-enable per the permissions doctrine.

## E. Iteration — `edit_action` v2

For ai-owned applets, `edit_action` gains the same structured params as setup (`agent`, `face_html`, `schema_sql`, `schedule`, …): it **rewrites the files**, re-checks, re-reconciles — disk stays source of truth, one door both directions. Live operational fields (`enabled`, `cron_schedule` nudges, `memory`) keep writing SQL directly as today. Whole-file rewrite is fine at these sizes (SEARCH/REPLACE editing deferred to the power tier). Hand-editing a compiled field sets `config.intent_stale = true`; the detail page shows a soft "description may be out of date" marker (one-way contract, per the overhaul plan).

## F. Grounding — `data_catalog` tool + AGENTS.md

- **`data_catalog` tool** (new, read-only, cheap): tables + columns + types + 3 sample rows, filterable by pattern. The authoring model calls it *before* writing SQL or prompts — catalog-check-first as doctrine, backed by a tool instead of hope. Also materialized to `actions/CATALOG.md` at reconcile for external agents (Claude Code / Cursor authoring against the repo).
- **`actions/AGENTS.md`** (replaces AUTHORING.md content for the authoring audience): the field contract, the capability table (§B), the three `until` idioms, canonical `[config.limits]` spellings, cron+timezone rules, the catch_up×window trap, decomposition rule (two cadences = two applets), honest-downgrade doctrine (no imaginary tables — *you are the check* for prose), and **complete exemplars**: `morning_examen` (Reflect), a one-shot reminder (`until = "once"`), `workout_nudge` (threshold Rule), Biscuit (face). Persona: explicitly marked **v2 — not yet authorable; offer draft-mode**.

## G. Acceptance suite

The ten writability-test asks (overhaul plan, Appendix B), authored end-to-end through real chat on a dev box. Pass = correct manifest materialized, check clean, gate shown, runs green (or the honest-downgrade behavior for #9/#10). This suite is the phase-3 definition of done.

## Sequence

1. **Capability allowlist + `data_catalog` tool** — small, unblocks everything, closes the runtime security gap properly.
2. **`--check` validators + API/CLI** — the compiler before the writer.
3. **`setup_action` v2 materialize + preview card + enable flow** — the flagship.
4. **`edit_action` v2 iteration + intent-staleness marker.**
5. **AGENTS.md + CATALOG.md + exemplars.**
6. **Acceptance suite runs; fix what fails.**

## Open questions (for the review pass)

- `face_html`/`schema_sql` as tool params: size limits and streaming ergonomics (a 300-line face through a tool-call arg — fine? chunked edits needed?).
- Owner=`ai` + seed-once reconcile vs. edit-rewrites-folder: exact field-ownership matrix for ai rows (which fields may edit_action write to disk vs SQL).
- Does `sql_query` hardening (face-reader pattern for ai-owned runs) land in phase 3 or ride with the capability-grant work?
- Grandfathered `action_agent_*` rows: any UI affordance to "upgrade to folder" or purely lazy on next edit?
- Estimated-cost model at the gate: static heuristic (slot price × schedule frequency) good enough for v1?
