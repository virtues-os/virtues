# Architecture

This document is the **implementation contract** for Virtues' applet system. It captures *what's true* and *why*. For the practical authoring guide, see [`applets/AUTHORING.md`](../applets/AUTHORING.md).

## TL;DR

**Applet** is the universal extension primitive in Virtues. Every applet lives in a folder at `applets/<name>/` with a `manifest.toml` declaring its metadata. Two runtime flavors:

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

> **Not an applet: privileged system daemons.** Infrastructure like the
> `virtues-wireguard` daemon (kernel `wg0`, `NET_ADMIN`, host networking) is
> **not** an applet — applets run in the app's *unprivileged* subprocess pool,
> spawned by the applet_runner; a rootless app can't spawn a `NET_ADMIN` child.
> Such daemons are launched by the *init system* (systemd) as their own
> units and coordinate with the app **through the DB**, not the applet dispatch
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

Trying to force these into one runtime (everything's a Lambda, or everything's an MCP server) makes the simple cases harder than they need to be. Distinct contracts cost dispatch code in core; collapsing them taxes every author of every applet forever.

The third need is real but is **not** served by an in-process supervisor: a supervisor that lives inside core dies with core, which is exactly wrong for work that must outlive a restart or upgrade. That is why supervised applets become systemd units rather than tokio children.

### Why not Docker

Considered and rejected for v1:

- **Docker-on-macOS** ships a 6GB Linux VM. For a self-hosted personal app, that's a meaningful regression.
- **Cold-start latency** of containerized cold-starts is ~200–500ms — *worse* than today's ~50–100ms subprocess fork. The "always-warm" feel requires keeping containers running, which has the same memory cost as the simpler `tokio::process` approach.
- **Image build pipelines, registries, layer caching** — all real complexity for a use case where the user is the only person installing extensions.

`tokio::process::Command` for fork-per-trigger work gives us what we need with zero dependency cost. For long-running work, systemd is already on the box and already solves supervision, restart, and boot ordering.

If at some future point we want sandboxing or untrusted code execution, that's a separate discussion. Self-hosted single-user means the trust boundary is the user's own filesystem.

### Why Applet is the parent name

The primitive was called **Action** until 2026-07. That name was chosen mainly because it was already in the codebase, and it was always weak for `view` — a chart is not an "action." The rename to **Applet** landed across the schema and the module tree; `app_applets`, `app_applet_runs`, `applet_runner`, `applet_templates`, and `AppletInput`/`AppletOutput` are the current names, and no `action` identifier survives in core.

Names considered and rejected, in the round that settled it:

- **Practices** — beautiful for the contemplative use cases, awkward for utilities (`housekeeping practice`?). Survives as the name of a gallery collection.
- **Offices** — monastic, exact fit for scheduled work, collides with workplace usage.
- **Instruments** — the best semantics on offer (means-of-action, instrument-panel, and the Rule of St. Benedict's "Instruments of Good Works" all land at once), but too long for the nav.
- **Artifact** — an inert output, not a runner. Also Anthropic's.
- **Daemons** — reads as techy or as demons, depending on the reader.
- **Keeper / Steward / Familiar** — personhood fails the moment the applet is a dashboard.
- **Droplets** — borrowed from DigitalOcean.

**Applet** wins on being unembarrassing in the nav while covering both runtimes. The namespace was checked and accepted as shared (IFTTT Applets, Alexa Skills, Apple Shortcuts, Home Assistant Automations). A persona panel found the deeper reason no name scored well: engineers name the mechanism, contemplatives name the meaning, and laypeople name only the instance — no single word wins all three camps, so the bar is "doesn't embarrass," not "delights."

User-facing UI uses runtime-specific words where they read better ("Functions", "Dashboards"); the parent noun appears in code and admin surfaces. "Vigil" is reserved as UI copy for watcher-shaped applets. Full decision record in [`applets-overhaul-plan.md`](./applets-overhaul-plan.md).

---

## Field ownership

The single most important architectural rule:

> **Manifest is declarative (what the applet *is*). SQL is operational (what state it's *in*). They don't share fields.**

| Field | Owner | Notes |
|---|---|---|
| `name`, `description`, `runtime`, `command`, `triggers`, `default_cron`, `default_enabled`, `per_credential`, `source`, `condition`, `agent`, `config` | **manifest.toml** | Manifest is canonical for system-owned applets; user-owned applets are seeded once via `INSERT OR IGNORE` and then the row belongs to the user. |
| current `enabled`, current `cron_schedule` (post-override), `last_run`, `runs[]`, `credential_id` (if fanned out), `created_at`, `updated_at` | **`app_applets` SQL** | Mutates with use. UI toggles + scheduler write here. Manifest never touches these. |

Reconcile is **unidirectional**: filesystem → SQL. SQL never writes to manifest. User-edits in the UI (toggle enabled, change cron schedule) write SQL only — your manifest is untouched. User-edits to a manifest propagate via reconcile but don't blow away SQL-owned runtime state.

There is no field where both could disagree. The "dual source of truth" worry doesn't apply.

---

## File layout

```
applets/
├── sources.toml                     # [[source]] catalog rows only — auth providers
├── Cargo.toml                       # [[bin]] entries for Rust function applets
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
│       ├── Card.svelte              # overrides TemplateCard for this applet
│       ├── Detail.svelte            # overrides AppletDetailView for this applet
│       └── Output.svelte            # (future) — renders run output anywhere it appears
└── ... (per-applet folders)

apps/web/src/lib/applet-views/
└── index.ts                         # Vite glob loader; discovers applets/*/ui/*.svelte at build time

/var/lib/virtues/applets/            # the WRITABLE applet root (state, not package data)
├── user/<slug>/                     # chat-authored applets — manifest.toml, schema.sql, face/
└── <imported-slug>/                 # Git packs cloned by /api/admin/applets/import-git
```

---

### Two applet roots

Applets resolve from two trees with opposite lifecycles:

| root | | |
|---|---|---|
| **shipped** — `/usr/local/share/virtues/applets` | package data: root-owned, read-only, replaced wholesale each release | `VIRTUES_APPLETS_DIR` |
| **state** — `/var/lib/virtues/applets` | user data: service-owned, written at runtime, never touched by the installer | `VIRTUES_APPLET_STATE_DIR` |

`resolve_applet_dir(dir)` checks state first, so an authored applet **shadows**
a shipped one of the same dir and deleting it reverts to shipped. Everything
written at runtime — chat authoring, Git pack import — goes to the state root
only.

They were one directory until authored applets started landing inside the tree
the installer replaces: a slot flip would delete them, and on a fresh box
authoring failed outright because nothing created a service-writable
directory. Reconcile's system-GC guard keys on the **shipped** root's template
count for the same reason — system rows come only from the shipped tree, so a
state root with one applet in it must not make a failed shipped load look like
a legitimately empty catalog.

---

## Dispatch flow

### `function` runtime

```
trigger fires (cron / webhook / manual)
  ↓
applet_runner::run_applet
  ↓ (resolves credential, evaluates condition, creates run row)
run_subprocess
  ↓ spawn child with stdin = AppletInput JSON
child reads stdin, does work, writes stdout = AppletOutput JSON, exits 0
  ↓
runner reads stdout, completes run row with summary
```

`command = [...]` is the argv to spawn. A bare `command[0]` (e.g. `["ios_ingest"]`) resolves to a Cargo-built applet binary under `target/{debug,release}/`; anything else runs via `PATH` (e.g. `["python3", "main.py"]`, `["node", "server.js"]`). The contract is language-agnostic.

### `view` runtime

No server-side dispatch. Applet runner returns `Skipped` for any cron/webhook/manual fire of a `view` applet. Scheduler excludes view runtimes from cron enqueue (no useless ticks).

Frontend dispatch:

```
TemplatesPanel renders applet card
  ↓ if applet.config?.view?.name has a registered Card.svelte → render it
  ↓ else fall back to generic TemplateCard

User clicks card → opens applet detail tab
  ↓ TabContent fetches applet by id
  ↓ if runtime == 'view' AND view.name has a registered Detail.svelte → render it
  ↓ else fall back to generic AppletDetailView
```

Vite-glob registry at `apps/web/src/lib/applet-views/index.ts` discovers `applets/<name>/ui/Card.svelte` / `Detail.svelte` files (co-located with the applet) at build time.

---

## Runtime contracts

### `function` contract

- **Stdin**: a JSON `AppletInput` with shape:
  ```json
  {
    "config": {...},                    // applet.config from SQL (manifest + overrides)
    "credentials": {...} | null,        // populated for applets with credential_id
    "payload": {...} | null             // trigger body (webhook request, manual args)
  }
  ```
- **Stdout**: a JSON `AppletOutput`:
  ```json
  {
    "result": "string summary",         // shown in run history
    "config": {...}                     // optional config update (cursor advance, etc.)
  }
  ```
- **Stderr**: free-form. Captured into `app_applet_runs.error` on non-zero exit.
- **Exit code**: 0 = success. Non-zero = failure; stderr becomes the error message.
- **Env**: master key (`VIRTUES_ENCRYPTION_KEY`) + `VIRTUES_DB_URL` typically. See [`crates/virtues-helpers/src/lib.rs`](../crates/virtues-helpers/src/lib.rs) for available helpers.

### `view` contract

- **No server-side execution.** Manifest's `triggers` should be `[]`.
- **Required**: `config.view.name` (the view bundle's lookup key — the folder name under `applets/<name>/ui/`).
- **Component shape**:
  - `Card.svelte` receives `{ applet: Applet, onclick?: (Applet) => void }` props.
  - `Detail.svelte` receives `{ tab: Tab }` props.
- **Data access**: views call core's HTTP API directly from the browser like any frontend page (with the user's session cookies).

---

## The reconcile loop

`reconcile_templates(db)` is the only function that writes manifest → SQL. It runs:

- On core boot (after migrations, before scheduler start)
- On `POST /api/admin/reconcile` (LLM authoring on-ramp)
- After credential mint/revoke (so per-credential fan-out updates)

Idempotency is required: back-to-back reconciles produce zero diffs. Verified by [`reconcile_is_idempotent`](../virtues-core/src/applet_templates/mod.rs) test.

The catalog (sources + per-applet manifests) is cached in an `OnceLock<RwLock<ParsedTemplates>>`. `reload_catalog()` re-globs from disk and replaces the inner state, so reconcile-on-demand picks up new manifests without restart.

---

## What we deliberately don't do (yet)

| Deferred | Why | Trigger to revisit |
|---|---|---|
| Docker / sandboxed runtime | Self-hosted single-user; user trusts their own code | First "install untrusted community applet" use case |
| Long-running (supervised) applets | In-process supervisor removed; systemd units are the replacement design | Applets overhaul reaches the supervised-work phase |
| Filesystem watcher (auto-reconcile on save) | Explicit reconcile is fine for v1 | Authoring volume justifies it (~once a day or more) |
| Heartbeat (`request_heartbeat: true` on AppletOutput) | Useful but unproven need | Multi-step agent loops feel rate-limited |
| Cross-platform hard memory caps (cgroups) | macOS-only RSS watchdog is best-effort | Real OOM risk in production |
| Recipe layer (high-level "create_applet(recipe, slots)" abstraction) | LLMs author fine via Write + Bash + reconcile | LLM authoring failure rate becomes problematic |
| Progressive disclosure for LLM tool exposure | Only matters at 50+ applets; we have ~20 | Applet count grows |
| Top-level dashboard page (UI surface for placed view widgets) | View runtime works without it | Demand for a personal dashboard surface |

---

## Operational surface

The `/applets` page surfaces:

- **Applets** — live list of `app_applets` rows (system + user + per-credential fan-out) in a filterable table: filter by Runtime (Function / View), Owner, Status, Trigger, Last run. A "Reconcile now" control re-syncs manifests → SQL.
- **Templates** — gallery of user-owned templates, card view. View-runtime applets show their custom Card.
- **History** — flat run-log across all applets.
- **Connections** — credential / source connection management.

Applet detail tab shows: header (name, description, status, controls) + body (config, runs, schedule editor).

---

## Where to find things

| What | Where |
|---|---|
| Manifest schema | [`applets/MANIFEST_SCHEMA.json`](../applets/MANIFEST_SCHEMA.json) |
| Authoring guide | [`applets/AUTHORING.md`](../applets/AUTHORING.md) |
| Applet manifest parser + reconcile | [`virtues-core/src/applet_templates/mod.rs`](../virtues-core/src/applet_templates/mod.rs) |
| Applet runner + dispatch | [`virtues-core/src/applet_runner/mod.rs`](../virtues-core/src/applet_runner/mod.rs) |
| Frontend view loader | [`apps/web/src/lib/applet-views/index.ts`](../apps/web/src/lib/applet-views/index.ts) |
| Applets page | [`apps/web/src/lib/components/applets/AppletsPanel.svelte`](../apps/web/src/lib/components/applets/AppletsPanel.svelte) |
| Admin reconcile endpoint | `POST /api/admin/reconcile` |
