# Authoring an Applet

This is the practical guide for adding a new applet to Virtues. For the implementation contract (how the runtime model works under the hood), see [`docs/architecture.md`](../docs/architecture.md). For schema validation, see [`MANIFEST_SCHEMA.json`](./MANIFEST_SCHEMA.json).

> **The whole story in one sentence.** An applet is a folder at `applets/<name>/` with a `manifest.toml` and (optionally) code or a Svelte component. Drop the folder, hit `POST /api/admin/reconcile`, and the applet is live.

---

## The three runtimes

Pick one based on what your applet actually does:

| Runtime | When to use | Example |
|---|---|---|
| **`function`** | Cron job, webhook handler, one-shot CLI | Sync data from an API; respond to an iOS webhook; run an LLM agent loop |
| **`view`** | Pure-frontend dashboard, no backend | Sleep chart over `data_health_*`; status tile on `/today`; widget |

If you're unsure, **start with `function`**. It's the simplest and covers ~80% of cases.

---

## Quick start by runtime

### `runtime = "function"` (Rust)

```
applets/
└── my_action/
    ├── manifest.toml
    └── main.rs
```

`applets/my_action/manifest.toml`:

```toml
name = "My Applet"
description = "What this does, in one sentence."
owner = "user"
runtime = "function"
command = ["my_action"]           # bare name → Cargo-built binary under target/
triggers = ["cron", "manual"]
default_cron = "0 */15 * * * *"   # every 15 minutes
```

`applets/my_action/main.rs`:

```rust
use anyhow::Result;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    let input = read_input()?;
    let _db = connect_from_env().await?;

    // Do work — read/write data_* tables, call APIs, run an agent.
    // input.config holds the applet's manifest config + user overrides.
    // input.credentials is populated for applets linked to a credential.
    // input.payload is the trigger body (webhook request or manual args).

    let summary = "ran ok";
    output(summary, &input.config)
}
```

`applets/Cargo.toml` — add the bin:

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

The applet appears in `/applets`, fires on its cron schedule.

### `runtime = "function"` (Python / Node / shell)

The contract is **stdin JSON in, stdout JSON out, exit 0 on success**. Any language works:

`manifest.toml`:

```toml
name = "My Python Applet"
description = "..."
owner = "user"
runtime = "function"
command = ["python3", "applets/my_python_action/main.py"]  # path relative to repo root
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
applets/
└── my_view/
    ├── manifest.toml
    └── ui/                   # UI co-located with the applet
        ├── Card.svelte       # optional — overrides TemplateCard
        └── Detail.svelte     # optional — overrides AppletDetailView
```

`applets/my_view/manifest.toml`:

```toml
name = "My View"
description = "A pure-frontend dashboard."
owner = "user"
runtime = "view"
triggers = []                  # never invoked server-side
default_enabled = true

[config.view]
name = "my_view"               # the view bundle key — folder name under applets/<name>/ui/
```

`applets/my_view/ui/Card.svelte`:

```svelte
<script lang="ts">
  import type { Applet } from '$lib/api/client';
  let { applet }: { applet: Applet } = $props();
</script>

<div class="card">
  <h3>{applet.name}</h3>
  <!-- Read data_* tables via the API, render whatever you want. -->
</div>
```

After reconcile, this view replaces the generic `TemplateCard` for this applet on the Templates page. If you also drop `Detail.svelte`, it replaces `AppletDetailView` when the user clicks through.

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

Code changes (editing `main.rs`, etc.) require a `cargo build` first if Rust. For polyglot applets with `command = [...]` and live-reload tooling, no rebuild needed.

---

## Field ownership

Manifest is **declarative** — what the applet *is*. SQL (`app_actions`) holds **runtime state** — what it's doing right now. They never disagree because they own different fields:

| Field | Lives in | Wins on conflict |
|---|---|---|
| `name`, `description`, `runtime`, `command`, `triggers`, `default_cron`, `default_enabled`, `per_credential`, `source`, `condition`, `agent`, `config` | manifest.toml | manifest (system applets) / first-seed-only (user applets) |
| current `enabled`, current `cron_schedule`, last_run, runs[], `credential_id` (if fanned out) | SQL | always SQL |

User toggles via the UI (enable/disable, change cron) write SQL only — your manifest is unchanged. User edits to manifest.toml propagate via reconcile but don't blow away user-managed runtime state.

---

## per_credential fan-out

If your applet needs to run **once per connected account** (e.g. one Google Calendar sync per Google credential), declare:

```toml
per_credential = true
source = { id = "google" }
```

Reconcile materializes one `app_actions` row per active credential of that source. Each row gets `credential_id` set; runtime auto-injects the matching credential into `ActionInput.credentials` when the applet fires.

`webhook` triggers **require** `per_credential = true` so bearer auth resolves to an identity.

---

## Naming conventions

- Folder name → applet's `id_prefix` (`action_<folder>`) by default
- A bare `command` (e.g. `["my_action"]`) must match a `[[bin]]` in `applets/Cargo.toml` for Rust; a multi-element `command` runs via `PATH`
- `runtime = "view"` applets must NOT set `command`
- `id_prefix` override is rarely needed; only set it if migrating an existing applet

---

## What gets invoked when

| Applet runtime | Cron tick | Webhook POST `/webhook/:id` | Manual fire | UI render |
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

- **Forgot to `cargo build`** — for Rust function applets, the binary must exist before reconcile. Builds happen at the workspace level: `cargo build --bin <bin-name>`.
- **`webhook` trigger without `per_credential`** — reconcile will refuse the manifest. Webhooks need a credential to authenticate.
- **`view` applet with `triggers = ["cron"]`** — won't run (scheduler skips view-runtime). Set `triggers = []`.
- **Live editing `main.rs`** — reconcile doesn't trigger `cargo build`. You still need to rebuild manually for Rust changes.

---

## Where things live

```
applets/
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
