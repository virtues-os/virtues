# Architecture

This document is the **implementation contract** for Virtues' multi-runtime action system. It captures *what's true* and *why*. For the practical authoring guide, see [`actions/AUTHORING.md`](./actions/AUTHORING.md).

## TL;DR

**Action** is the universal extension primitive in Virtues. Every action lives in a folder at `actions/<name>/` with a `manifest.toml` declaring its metadata. Three runtime flavors:

- **`function`** — Lambda-style. Fork-per-trigger CLI (any language). JSON in/out via stdin/stdout.
- **`service`** — Heroku-style. Long-running supervised HTTP server (any language). Routed by core at `/service/<id>/*`.
- **`view`** — Pure Svelte component. No server-side execution.

Same authoring surface. Same dispatch infrastructure. Three contracts.

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

### Why three runtimes (not one, not many)

Personal-AI workloads split cleanly into three needs:

| Need | Example | Right shape |
|---|---|---|
| Cron-driven sync, occasional webhook handler | iOS HealthKit ingest, Plaid sync | Fork-per-trigger CLI |
| Latency-sensitive or persistent-connection backend | Hue light controller, MQTT subscriber, real-time chart | Long-running HTTP server |
| Pure dashboard / chart over already-ingested data | Sleep trends, mail counter | Svelte component, no backend |

Trying to force these into one runtime (everything's a Lambda, or everything's a service, or everything's an MCP server) makes the simple cases harder than they need to be. Three distinct contracts costs ~600 LOC of supervisor + dispatch in core; collapsing them taxes every author of every action forever.

### Why not Docker

Considered and rejected for v1:

- **Docker-on-macOS** ships a 6GB Linux VM. For a self-hosted personal app, that's a meaningful regression.
- **Cold-start latency** of containerized cold-starts is ~200–500ms — *worse* than today's ~50–100ms subprocess fork. The "always-warm" feel requires keeping containers running, which has the same memory cost as the simpler `tokio::process` approach.
- **Image build pipelines, registries, layer caching** — all real complexity for a use case where the user is the only person installing extensions.

`tokio::process::Command` + an axum HTTP proxy + a small port allocator gives us 95% of what Docker-style supervision provides, with zero dependency cost.

If at some future point we want sandboxing or untrusted code execution, that's a separate discussion. Self-hosted single-user means the trust boundary is the user's own filesystem.

### Why Action is the parent name

We considered "Practices," "Offices," "Droplets," "Extensions." Each has rough edges:

- **Practices** — beautiful for the contemplative use cases, awkward for utilities (`housekeeping practice`?).
- **Offices** — monastic, exact fit for scheduled work, collides with workplace usage.
- **Droplets** — borrowed from DigitalOcean.
- **Extensions** — VS Code precedent; good but expensive rename across ~50 files.

"Action" works because it's already in the codebase (`app_actions` table, `action_runner`, etc.) and it's neutral enough to cover all three runtimes. It's slightly weak for `view` (a chart isn't really an "action"), but every alternative has a worse fit somewhere. We accept the rough edge.

User-facing UI uses runtime-specific words where they read better ("Functions", "Services", "Dashboards") — the parent noun appears in code and admin surfaces only.

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
├── Cargo.toml                       # [[bin]] entries for Rust function/service actions
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
├── echo_app/
│   ├── manifest.toml                # runtime = "service"
│   └── main.rs
├── hello_world/
│   ├── manifest.toml                # runtime = "view"
│   └── ui/
│       ├── Card.svelte              # overrides TemplateCard for this action
│       ├── Detail.svelte            # overrides ActionDetailView for this action
│       └── Output.svelte            # (future) — renders run output anywhere it appears
└── ... (per-action folders)

apps/web/src/lib/action-views/
└── index.ts                         # Vite glob loader; discovers actions/*/ui/*.svelte at build time

virtues-core/src/services/                   # the service-runtime supervisor
├── mod.rs
├── registry.rs                      # in-memory `action_id → RunningService`, log ring buffers
├── supervisor.rs                    # spawn / watch / restart / shutdown
└── proxy.rs                         # axum reverse-proxy for /service/<id>/*
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

### `service` runtime

```
core boots
  ↓
ServiceSupervisor::start
  ↓ for each runtime='service' action:
       allocate port, spawn child, capture stdout/stderr to ring buffer
       health-probe /__health for 5s → mark Running
  ↓
external HTTP request to /service/<id>/<path>
  ↓
proxy::handle_service_proxy
  ↓ look up port from registry
  ↓ forward request via reqwest, stream response back
```

For cron / webhook / manual triggers on a `service`-runtime action, the runner POSTs the `ActionInput` to `/service/<id>/__trigger` (via the same proxy). 404 → action doesn't handle that trigger style (treated as a no-op, not an error).

Crash + restart loop:

```
child exits unexpectedly → watchdog task records via mpsc → restart loop applies
exponential backoff [1s, 2s, 5s, 15s, 60s, 300s cap] → respawn
after MAX_RESTARTS (10) consecutive failures → mark Crashed, manual reconcile required
```

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

### `service` contract

- **Spawned with env**:
  - `PORT` — bind here (allocated by supervisor, starts at 3100, sequential)
  - `VIRTUES_API_BASE` — call core's API at this URL (typically `http://127.0.0.1:8000`)
  - `VIRTUES_ACTION_ID` — your action's id (for log correlation)
- **Conventions** (optional but supervisor-aware):
  - `GET /__health` — required for the readiness probe; supervisor polls until 2xx for 5s. Failed probe → service stays in `Starting`, traffic returns 503.
  - `POST /__trigger` — fired when the action is invoked via cron/webhook/manual. Body is `ActionInput` JSON. 404 → treated as no-op (not an error).
- **Lifecycle**: spawn at boot, watch for exit, restart on crash with exponential backoff, SIGTERM on shutdown, SIGKILL after 3s drain.
- **Auth (v1)**: localhost trust. No service token. The service calls `VIRTUES_API_BASE/api/...` with no bearer; core's API allows unauthenticated requests when reached via 127.0.0.1.

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

After SQL reconcile, the API handler also calls `ServiceSupervisor::reload(db)` to diff the running service processes:

- Service in DB but not running → spawn
- Service running but not in DB / disabled → stop and remove
- Service in DB and Crashed (exceeded MAX_RESTARTS) → drop registry slot and respawn fresh — lets the user fix code, hit reconcile, recover
- (v1.1) Both, command/config changed → restart with new args (today, edit a manifest's `command` or `config` and toggle enabled off→on, or restart core)

---

## What we deliberately don't do (yet)

| Deferred | Why | Trigger to revisit |
|---|---|---|
| Docker / sandboxed runtime | Self-hosted single-user; user trusts their own code | First "install untrusted community action" use case |
| Filesystem watcher (auto-reconcile on save) | Explicit reconcile is fine for v1 | Authoring volume justifies it (~once a day or more) |
| MCP support on `service` runtime | Substrate works without it; existing MCP servers can be wrapped if needed | Want to install community MCP servers as actions |
| Heartbeat (`request_heartbeat: true` on ActionOutput) | Useful but unproven need | Multi-step agent loops feel rate-limited |
| Per-app token issuance | Localhost trust is sufficient | Multi-user or sandboxed runtime |
| Hot-reload of running apps on file change | Restart core picks up code changes | Iteration friction becomes painful |
| Cross-platform hard memory caps (cgroups) | macOS-only RSS watchdog is best-effort | Real OOM risk in production |
| Recipe layer (high-level "create_action(recipe, slots)" abstraction) | LLMs author fine via Write + Bash + reconcile | LLM authoring failure rate becomes problematic |
| Progressive disclosure for LLM tool exposure | Only matters at 50+ actions; we have ~20 | Action count grows |
| Top-level dashboard page (UI surface for placed view widgets) | View runtime works without it | Demand for a personal dashboard surface |
| Per-credential `service`-runtime fan-out | Single-user single-instance is sufficient | Multi-account use case |
| WebSocket / HTTP Upgrade through the service proxy | No current service needs it; proxy is HTTP/1.1 request-response only | Real-time UI in a `service`-runtime action |
| `stop_one` SIGTERM during reload | Child moved into watchdog task; relies on `kill_on_drop` at supervisor teardown. Removed services keep serving until core restart. | Manifest churn becomes routine |

---

## Operational surface

The `/actions` page surfaces:

- **Actions** — live list of `app_actions` rows (system + user + per-credential fan-out) in a filterable table: filter by Runtime (Function / Service / View), Owner, Status, Trigger, Last run. Service-runtime rows carry supervisor state (port, status, restart count); a "Reconcile now" control re-syncs manifests → SQL and respawns services.
- **Templates** — gallery of user-owned templates, card view. View-runtime actions show their custom Card.
- **History** — flat run-log across all actions.
- **Connections** — credential / source connection management.

Action detail tab shows: header (name, description, status, controls) + body (config, runs, schedule editor) + (for `runtime = "service"` only) **Logs panel** tailing stdout/stderr from the supervisor's per-service ring buffer.

---

## Where to find things

| What | Where |
|---|---|
| Manifest schema | [`actions/MANIFEST_SCHEMA.json`](./actions/MANIFEST_SCHEMA.json) |
| Authoring guide | [`actions/AUTHORING.md`](./actions/AUTHORING.md) |
| Action manifest parser + reconcile | [`virtues-core/src/action_templates/mod.rs`](./virtues-core/src/action_templates/mod.rs) |
| Action runner + dispatch | [`virtues-core/src/action_runner/mod.rs`](./virtues-core/src/action_runner/mod.rs) |
| Service supervisor | [`virtues-core/src/services/supervisor.rs`](./virtues-core/src/services/supervisor.rs) |
| Service proxy | [`virtues-core/src/services/proxy.rs`](./virtues-core/src/services/proxy.rs) |
| Service registry + log ring buffer | [`virtues-core/src/services/registry.rs`](./virtues-core/src/services/registry.rs) |
| Frontend view loader | [`apps/web/src/lib/action-views/index.ts`](./apps/web/src/lib/action-views/index.ts) |
| Actions page (incl. supervisor view) | [`apps/web/src/lib/components/actions/ActionsPanel.svelte`](./apps/web/src/lib/components/actions/ActionsPanel.svelte) |
| Logs panel | [`apps/web/src/lib/components/actions/LogsPanel.svelte`](./apps/web/src/lib/components/actions/LogsPanel.svelte) |
| Admin reconcile endpoint | `POST /api/admin/reconcile` |
