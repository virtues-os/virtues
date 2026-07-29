# Authoring an Action

This is the practical guide for adding a new action to Virtues. For the implementation contract (how the runtime model works under the hood), see [`ARCHITECTURE.md`](../ARCHITECTURE.md). For schema validation, see [`MANIFEST_SCHEMA.json`](./MANIFEST_SCHEMA.json).

> **The whole story in one sentence.** An action is a folder at `actions/<name>/` with a `manifest.toml` and (optionally) code or a Svelte component. Drop the folder, hit `POST /api/admin/reconcile`, and the action is live.

---

## The three runtimes

Pick one based on what your action actually does:

| Runtime | When to use | Example |
|---|---|---|
| **`function`** | Cron job, webhook handler, one-shot CLI | Sync data from an API; respond to an iOS webhook; run an LLM agent loop |
| **`view`** | Pure-frontend dashboard, no backend | Sleep chart over `data_health_*`; status tile on `/today`; widget |

If you're unsure, **start with `function`**. It's the simplest and covers ~80% of cases.

---

## Quick start by runtime

### `runtime = "function"` (Rust)

```
actions/
└── my_action/
    ├── manifest.toml
    └── main.rs
```

`actions/my_action/manifest.toml`:

```toml
name = "My Action"
description = "What this does, in one sentence."
owner = "user"
runtime = "function"
command = ["my_action"]           # bare name → Cargo-built binary under target/
triggers = ["cron", "manual"]
default_cron = "0 */15 * * * *"   # every 15 minutes
```

`actions/my_action/main.rs`:

```rust
use anyhow::Result;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    let input = read_input()?;
    let _db = connect_from_env().await?;

    // Do work — read/write data_* tables, call APIs, run an agent.
    // input.config holds the action's manifest config + user overrides.
    // input.credentials is populated for actions linked to a credential.
    // input.payload is the trigger body (webhook request or manual args).

    let summary = "ran ok";
    output(summary, &input.config)
}
```

`actions/Cargo.toml` — add the bin:

```toml
[[bin]]
name = "my_action"
path = "my_action/main.rs"
```

Build + reconcile:

```bash
cargo build --bin my_action
curl -X POST http://localhost:8000/api/admin/reconcile
```

The action appears in `/actions`, fires on its cron schedule.

### `runtime = "function"` (Python / Node / shell)

The contract is **stdin JSON in, stdout JSON out, exit 0 on success**. Any language works:

`manifest.toml`:

```toml
name = "My Python Action"
description = "..."
owner = "user"
runtime = "function"
command = ["python3", "actions/my_python_action/main.py"]  # path relative to repo root
triggers = ["cron"]
default_cron = "0 */30 * * * *"
```

`main.py`:

```python
#!/usr/bin/env python3
import json, sys

inp = json.load(sys.stdin)
# inp = { "config": {...}, "credentials": {...} | None, "payload": ... | None }

# do the work...
result = "synced 42 records"

print(json.dumps({"result": result, "config": inp["config"]}))
```

No Cargo entry needed — a multi-element `command` runs the script directly via `PATH`. Reconcile picks it up after the folder lands.

### `runtime = "view"`

```
actions/
└── my_view/
    ├── manifest.toml
    └── ui/                   # UI co-located with the action
        ├── Card.svelte       # optional — overrides TemplateCard
        └── Detail.svelte     # optional — overrides AppletDetailView
```

`actions/my_view/manifest.toml`:

```toml
name = "My View"
description = "A pure-frontend dashboard."
owner = "user"
runtime = "view"
triggers = []                  # never invoked server-side
default_enabled = true

[config.view]
name = "my_view"               # the view bundle key — folder name under actions/<name>/ui/
```

`actions/my_view/ui/Card.svelte`:

```svelte
<script lang="ts">
  import type { Action } from '$lib/api/client';
  let { action }: { action: Action } = $props();
</script>

<div class="card">
  <h3>{action.name}</h3>
  <!-- Read data_* tables via the API, render whatever you want. -->
</div>
```

After reconcile, this view replaces the generic `TemplateCard` for this action on the Templates page. If you also drop `Detail.svelte`, it replaces `AppletDetailView` when the user clicks through.

---

## Reconcile

Anytime you create, edit, or delete a manifest on disk, tell core to pick up the change:

```bash
curl -X POST http://localhost:8000/api/admin/reconcile
```

Returns:

```json
{
  "upserted": 21,        // total app_actions rows refreshed
  "restarted": []        // (v1.1)
}
```

Code changes (editing `main.rs`, etc.) require a `cargo build` first if Rust. For polyglot actions with `command = [...]` and live-reload tooling, no rebuild needed.

---

## Field ownership

Manifest is **declarative** — what the action *is*. SQL (`app_actions`) holds **runtime state** — what it's doing right now. They never disagree because they own different fields:

| Field | Lives in | Wins on conflict |
|---|---|---|
| `name`, `description`, `runtime`, `command`, `triggers`, `default_cron`, `default_enabled`, `per_credential`, `source`, `condition`, `agent`, `config` | manifest.toml | manifest (system actions) / first-seed-only (user actions) |
| current `enabled`, current `cron_schedule`, last_run, runs[], `credential_id` (if fanned out) | SQL | always SQL |

User toggles via the UI (enable/disable, change cron) write SQL only — your manifest is unchanged. User edits to manifest.toml propagate via reconcile but don't blow away user-managed runtime state.

---

## per_credential fan-out

If your action needs to run **once per connected account** (e.g. one Google Calendar sync per Google credential), declare:

```toml
per_credential = true
source = { id = "google" }
```

Reconcile materializes one `app_actions` row per active credential of that source. Each row gets `credential_id` set; runtime auto-injects the matching credential into `ActionInput.credentials` when the action fires.

`webhook` triggers **require** `per_credential = true` so bearer auth resolves to an identity.

---

## Naming conventions

- Folder name → action's `id_prefix` (`action_<folder>`) by default
- A bare `command` (e.g. `["my_action"]`) must match a `[[bin]]` in `actions/Cargo.toml` for Rust; a multi-element `command` runs via `PATH`
- `runtime = "view"` actions must NOT set `command`
- `id_prefix` override is rarely needed; only set it if migrating an existing action

---

## What gets invoked when

| Action runtime | Cron tick | Webhook POST `/webhook/:id` | Manual fire | UI render |
|---|---|---|---|---|
| `function` | spawns subprocess | spawns subprocess | spawns subprocess | shows generic card |
| `view` | skipped (never enqueued) | n/a | skipped | renders custom Card.svelte |

For `function`, the trigger payload is delivered as JSON on stdin.

---

## When to choose what

| You want | Pick | Why |
|---|---|---|
| Run every N minutes/hours | `function` + `default_cron` | Lowest overhead; fork-per-trigger is fine |
| React to a device webhook | `function` + `triggers = ["webhook"]` + `per_credential` | Standard pattern for iOS/Mac streams |
| Multi-step LLM agent loop | `function` + `agent = "..."` (no `command`) | Built-in agent runner |
| Dashboard of `data_*` tables | `view` | No backend needed; pure SQL + Svelte |
| Polyglot script (Python, Node, Bash) | `function` + `command = [...]` | Cargo not involved |

---

## Common pitfalls

- **Forgot to `cargo build`** — for Rust function actions, the binary must exist before reconcile. Builds happen at the workspace level: `cargo build --bin <bin-name>`.
- **`webhook` trigger without `per_credential`** — reconcile will refuse the manifest. Webhooks need a credential to authenticate.
- **`view` action with `triggers = ["cron"]`** — won't run (scheduler skips view-runtime). Set `triggers = []`.
- **Live editing `main.rs`** — reconcile doesn't trigger `cargo build`. You still need to rebuild manually for Rust changes.

---

## Where things live

```
actions/
├── sources.toml                     # [[source]] catalog (auth providers)
├── Cargo.toml                       # [[bin]] entries for Rust functions
├── <name>/
│   ├── manifest.toml                # declarative metadata
│   ├── main.rs                      # function entry (Rust)
│   ├── transform.rs                 # ... or whatever helper modules
│   ├── ui/                          # for runtime = "view"
│   │   ├── Card.svelte
│   │   └── Detail.svelte
│   └── ...
└── MANIFEST_SCHEMA.json             # validate your manifest against this
```

That's the whole story.
