# Architecture

This document is the **implementation contract** for Virtues' action system. It captures *what's true* and *why*. For the practical authoring guide, see [`actions/AUTHORING.md`](./actions/AUTHORING.md).

## TL;DR

**Action** is the universal extension primitive in Virtues. Every action lives in a folder at `actions/<name>/` with a `manifest.toml` declaring its metadata. Two runtime flavors:

- **`function`** — Lambda-style. Fork-per-trigger CLI (any language). JSON in/out via stdin/stdout.
- **`view`** — Pure Svelte component. No server-side execution.

Same authoring surface. Same dispatch infrastructure. Two contracts.

> **Long-running applets.** A third `service` runtime — an in-process tokio
> supervisor with a port allocator and a `/service/<id>` reverse proxy — was
> built and removed. It shipped one applet (a demo), never ran successfully on
> a box, and the supervised-work design moved to systemd units
> (`virtues-applet-<id>`); see [`applets-overhaul-plan.md`](./applets-overhaul-plan.md).
> The rationale below for *why* long-running work needs a different shape than
> fork-per-trigger still holds — only the mechanism changed.

> **Not an action: privileged system daemons.** Infrastructure like the
> `virtues-wireguard` daemon (kernel `wg0`, `NET_ADMIN`, host networking) is
> **not** an action — actions run in the app's *unprivileged* subprocess pool,
> spawned by the action_runner; a rootless app can't spawn a `NET_ADMIN` child.
> Such daemons are launched by the *init system* (systemd) as their own
> units and coordinate with the app **through the DB**, not the action dispatch
> path. Same "small composable process" spirit, separate privilege/lifecycle
> tier. See [`deployment.md`](./deployment.md).

---

## Why this shape

### Why two runtimes (not one, not many)

Personal-AI workloads split cleanly into distinct needs:

| Need | Example | Right shape |
|---|---|---|
| Cron-driven sync, occasional webhook handler | iOS HealthKit ingest, Plaid sync | Fork-per-trigger CLI |
| Pure dashboard / chart over already-ingested data | Sleep trends, mail counter | Svelte component, no backend |
| Latency-sensitive or persistent-connection backend | Hue light controller, MQTT subscriber | systemd unit (deferred) |

Trying to force these into one runtime (everything's a Lambda, or everything's an MCP server) makes the simple cases harder than they need to be. Distinct contracts cost dispatch code in core; collapsing them taxes every author of every action forever.

The third need is real but is **not** served by an in-process supervisor: a supervisor that lives inside core dies with core, which is exactly wrong for work that must outlive a restart or upgrade. That is why supervised applets become systemd units rather than tokio children.

### Why not Docker

Considered and rejected for v1:

- **Docker-on-macOS** ships a 6GB Linux VM. For a self-hosted personal app, that's a meaningful regression.
- **Cold-start latency** of containerized cold-starts is ~200–500ms — *worse* than today's ~50–100ms subprocess fork. The "always-warm" feel requires keeping containers running, which has the same memory cost as the simpler `tokio::process` approach.
- **Image build pipelines, registries, layer caching** — all real complexity for a use case where the user is the only person installing extensions.

`tokio::process::Command` for fork-per-trigger work gives us what we need with zero dependency cost. For long-running work, systemd is already on the box and already solves supervision, restart, and boot ordering.

If at some future point we want sandboxing or untrusted code execution, that's a separate discussion. Self-hosted single-user means the trust boundary is the user's own filesystem.

### Why Action is the parent name

We considered "Practices," "Offices," "Droplets," "Extensions." Each has rough edges:

- **Practices** — beautiful for the contemplative use cases, awkward for utilities (`housekeeping practice`?).
- **Offices** — monastic, exact fit for scheduled work, collides with workplace usage.
- **Droplets** — borrowed from DigitalOcean.
- **Extensions** — VS Code precedent; good but expensive rename across ~50 files.

"Action" works because it's already in the codebase (`app_actions` table, `action_runner`, etc.) and it's neutral enough to cover both runtimes. It's slightly weak for `view` (a chart isn't really an "action"), but every alternative has a worse fit somewhere. We accept the rough edge.

User-facing UI uses runtime-specific words where they read better ("Functions", "Dashboards") — the parent noun appears in code and admin surfaces only.

---

## Field ownership

The single most important architectural rule:

> **Manifest is declarative (what the action *is*). SQL is operational (what state it's *in*). They don't share fields.**

| Field | Owner | Notes |
|---|---|---|
| `name`, `description`, `runtime`, `command`, `triggers`, `default_cron`, `default_enabled`, `per_credential`, `source`, `condition`, `agent`, `config` | **manifest.toml** | Manifest is canonical for system-owned actions; user-owned actions are seeded once via `INSERT OR IGNORE` and then the row belongs to the user. |
| current `enabled`, current `cron_schedule` (post-override), `last_run`, `runs[]`, `credential_id` (if fanned out), `created_at`, `updated_at` | **`app_actions` SQL** | Mutates with use. UI toggles + scheduler write here. Manifest never touches these. |

Reconcile is **unidirectional**: filesystem → SQL. SQL never writes to manifest. User-edits in the UI (toggle enabled, change cron schedule) write SQL only — your manifest is untouched. User-edits to a manifest propagate via reconcile but don't blow away SQL-owned runtime state.

There is no field where both could disagree. The "dual source of truth" worry doesn't apply.

---

## File layout

```
actions/
├── sources.toml                     # [[source]] catalog rows only — auth providers
├── Cargo.toml                       # [[bin]] entries for Rust function actions
├── MANIFEST_SCHEMA.json             # JSON Schema for manifest.toml — LLM-validatable
├── AUTHORING.md                     # practical guide (link target)
├── ios_ingest/                      # one binary for all paired-iPhone streams
│   ├── manifest.toml
│   ├── main.rs                       # dispatches on the body `stream` field
│   ├── healthkit.rs                  # one module per stream
│   ├── location.rs
│   └── …
├── morning_examen/
│   └── manifest.toml                # agent-only; no binary
├── hello_world/
│   ├── manifest.toml                # runtime = "view"
│   └── ui/
│       ├── Card.svelte              # overrides TemplateCard for this action
│       ├── Detail.svelte            # overrides ActionDetailView for this action
│       └── Output.svelte            # (future) — renders run output anywhere it appears
└── ... (per-action folders)

apps/web/src/lib/action-views/
└── index.ts                         # Vite glob loader; discovers actions/*/ui/*.svelte at build time
```

---

## Dispatch flow

### `function` runtime

```
trigger fires (cron / webhook / manual)
  ↓
action_runner::run_action
  ↓ (resolves credential, evaluates condition, creates run row)
run_subprocess
  ↓ spawn child with stdin = ActionInput JSON
child reads stdin, does work, writes stdout = ActionOutput JSON, exits 0
  ↓
runner reads stdout, completes run row with summary
```

`command = [...]` is the argv to spawn. A bare `command[0]` (e.g. `["ios_ingest"]`) resolves to a Cargo-built action binary under `target/{debug,release}/`; anything else runs via `PATH` (e.g. `["python3", "main.py"]`, `["node", "server.js"]`). The contract is language-agnostic.

### `view` runtime

No server-side dispatch. Action runner returns `Skipped` for any cron/webhook/manual fire of a `view` action. Scheduler excludes view runtimes from cron enqueue (no useless ticks).

Frontend dispatch:

```
TemplatesPanel renders action card
  ↓ if action.config?.view?.name has a registered Card.svelte → render it
  ↓ else fall back to generic TemplateCard

User clicks card → opens action detail tab
  ↓ TabContent fetches action by id
  ↓ if runtime == 'view' AND view.name has a registered Detail.svelte → render it
  ↓ else fall back to generic ActionDetailView
```

Vite-glob registry at `apps/web/src/lib/action-views/index.ts` discovers `actions/<name>/ui/Card.svelte` / `Detail.svelte` files (co-located with the action) at build time.

---

## Runtime contracts

### `function` contract

- **Stdin**: a JSON `ActionInput` with shape:
  ```json
  {
    "config": {...},                    // action.config from SQL (manifest + overrides)
    "credentials": {...} | null,        // populated for actions with credential_id
    "payload": {...} | null             // trigger body (webhook request, manual args)
  }
  ```
- **Stdout**: a JSON `ActionOutput`:
  ```json
  {
    "result": "string summary",         // shown in run history
    "config": {...}                     // optional config update (cursor advance, etc.)
  }
  ```
- **Stderr**: free-form. Captured into `app_action_runs.error` on non-zero exit.
- **Exit code**: 0 = success. Non-zero = failure; stderr becomes the error message.
- **Env**: master key (`VIRTUES_ENCRYPTION_KEY`) + `VIRTUES_DB_URL` typically. See [`crates/virtues-helpers/src/lib.rs`](./crates/virtues-helpers/src/lib.rs) for available helpers.

### `view` contract

- **No server-side execution.** Manifest's `triggers` should be `[]`.
- **Required**: `config.view.name` (the view bundle's lookup key — the folder name under `actions/<name>/ui/`).
- **Component shape**:
  - `Card.svelte` receives `{ action: Action, onclick?: (Action) => void }` props.
  - `Detail.svelte` receives `{ tab: Tab }` props.
- **Data access**: views call core's HTTP API directly from the browser like any frontend page (with the user's session cookies).

---

## The reconcile loop

`reconcile_templates(db)` is the only function that writes manifest → SQL. It runs:

- On core boot (after migrations, before scheduler start)
- On `POST /api/admin/reconcile` (LLM authoring on-ramp)
- After credential mint/revoke (so per-credential fan-out updates)

Idempotency is required: back-to-back reconciles produce zero diffs. Verified by [`reconcile_is_idempotent`](./virtues-core/src/action_templates/mod.rs) test.

The catalog (sources + per-action manifests) is cached in an `OnceLock<RwLock<ParsedTemplates>>`. `reload_catalog()` re-globs from disk and replaces the inner state, so reconcile-on-demand picks up new manifests without restart.

---

## What we deliberately don't do (yet)

| Deferred | Why | Trigger to revisit |
|---|---|---|
| Docker / sandboxed runtime | Self-hosted single-user; user trusts their own code | First "install untrusted community action" use case |
| Long-running (supervised) applets | In-process supervisor removed; systemd units are the replacement design | Applets overhaul reaches the supervised-work phase |
| Filesystem watcher (auto-reconcile on save) | Explicit reconcile is fine for v1 | Authoring volume justifies it (~once a day or more) |
| Heartbeat (`request_heartbeat: true` on ActionOutput) | Useful but unproven need | Multi-step agent loops feel rate-limited |
| Cross-platform hard memory caps (cgroups) | macOS-only RSS watchdog is best-effort | Real OOM risk in production |
| Recipe layer (high-level "create_action(recipe, slots)" abstraction) | LLMs author fine via Write + Bash + reconcile | LLM authoring failure rate becomes problematic |
| Progressive disclosure for LLM tool exposure | Only matters at 50+ actions; we have ~20 | Action count grows |
| Top-level dashboard page (UI surface for placed view widgets) | View runtime works without it | Demand for a personal dashboard surface |

---

## Operational surface

The `/actions` page surfaces:

- **Actions** — live list of `app_applets` rows (system + user + per-credential fan-out) in a filterable table: filter by Runtime (Function / View), Owner, Status, Trigger, Last run. A "Reconcile now" control re-syncs manifests → SQL.
- **Templates** — gallery of user-owned templates, card view. View-runtime actions show their custom Card.
- **History** — flat run-log across all actions.
- **Connections** — credential / source connection management.

Action detail tab shows: header (name, description, status, controls) + body (config, runs, schedule editor).

---

## Where to find things

| What | Where |
|---|---|
| Manifest schema | [`actions/MANIFEST_SCHEMA.json`](./actions/MANIFEST_SCHEMA.json) |
| Authoring guide | [`actions/AUTHORING.md`](./actions/AUTHORING.md) |
| Action manifest parser + reconcile | [`virtues-core/src/action_templates/mod.rs`](./virtues-core/src/action_templates/mod.rs) |
| Action runner + dispatch | [`virtues-core/src/action_runner/mod.rs`](./virtues-core/src/action_runner/mod.rs) |
| Frontend view loader | [`apps/web/src/lib/action-views/index.ts`](./apps/web/src/lib/action-views/index.ts) |
| Actions page | [`apps/web/src/lib/components/actions/ActionsPanel.svelte`](./apps/web/src/lib/components/actions/ActionsPanel.svelte) |
| Admin reconcile endpoint | `POST /api/admin/reconcile` |
