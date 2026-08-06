# Authoring an Applet

This is the guide for **builtin applets — the ones that ship with the box and run compiled code.** If you are writing an applet from chat, or anything declarative (a prompt, a schedule, a face), [`AGENTS.md`](./AGENTS.md) is the contract you want. For schema validation see [`MANIFEST_SCHEMA.json`](./MANIFEST_SCHEMA.json).

> **The whole story in one sentence.** An applet is a folder at `applets/<name>/` with a `manifest.toml` and, optionally, code or a face. Drop the folder, hit `POST /api/admin/reconcile`, and the applet is live.

---

## There is no runtime field

An applet's shape is **derived from which fields you set**, never declared. There was a `runtime` key once; it is gone, because a manifest could declare one thing and set fields that meant another, and the declaration won — a manifest with a command and `runtime = "view"` passed validation and then never ran.

| You set | What it is |
|---|---|
| `command` | a subprocess: forked per trigger, stdin/stdout JSON |
| `command` + `supervise = true` | a long-lived supervised service |
| `agent` | an LLM agent loop (runs after the subprocess phase, if both are set) |
| a `face/index.html` and neither of the above | a face-only applet — never invoked server-side, so `triggers = []` |

Reconcile refuses a manifest that runs (has a command or an agent) with an empty `triggers` list, because it could never fire.

---

## Quick start

### A subprocess applet (Rust)

```
applets/
└── my_applet/
    ├── manifest.toml
    └── main.rs
```

`applets/my_applet/manifest.toml`:

```toml
name = "My Applet"
description = "What this does, in one sentence."
owner = "user"
command = ["my_applet"]           # bare name → Cargo-built binary under target/
triggers = ["cron", "manual"]
schedule = "0 */15 * * * *"       # every 15 minutes, box-local time
```

`applets/my_applet/main.rs`:

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
name = "my_applet"
path = "my_applet/main.rs"
```

Build + reconcile:

```bash
cargo build --bin my_applet
curl -X POST http://localhost:8000/api/admin/reconcile
```

The applet appears in `/applets`, fires on its cron schedule.

### A subprocess applet (Python / Node / shell)

The contract is **stdin JSON in, stdout JSON out, exit 0 on success**. Any language works:

`manifest.toml`:

```toml
name = "My Python Applet"
description = "..."
owner = "user"
command = ["python3", "applets/my_python_applet/main.py"]  # path relative to repo root
triggers = ["cron"]
schedule = "0 */30 * * * *"
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

### A face-only applet

A face is `face/index.html`, rendered in a sandboxed iframe. There is no Svelte
option: an applet folder must not mount components inside the app bundle, which
is the whole point of the iframe's opaque origin.

```
applets/
└── my_dashboard/
    ├── manifest.toml
    └── face/
        └── index.html
```

```toml
name = "My Dashboard"
description = "What this shows you, in one sentence."
owner = "user"
triggers = []                  # never invoked server-side
default_enabled = true
```

Inside `index.html`, link `virtues.css` for the box's theme and use
`await virtues.query(sql)` for read-only access to your data. See
`applets/hello_world/face/index.html` for a working one.

---

## Reconcile

Anytime you create, edit, or delete a manifest on disk, tell core to pick up the change:

```bash
curl -X POST http://localhost:8000/api/admin/reconcile
```

Returns:

```json
{
  "upserted": 24,        // total app_applets rows refreshed
  "restarted": []        // (v1.1)
}
```

Code changes (editing `main.rs`, etc.) require a `cargo build` first if Rust. For polyglot applets with `command = [...]` and live-reload tooling, no rebuild needed.

---

## Field ownership

Manifest is **declarative** — what the applet *is*. SQL (`app_applets`) holds **runtime state** — what it's doing right now. They never disagree because they own different fields:

| Field | Lives in | Wins on conflict |
|---|---|---|
| `name`, `description`, `command`, `triggers`, `schedule`, `default_enabled`, `per_credential`, `source`, `condition`, `until`, `agent`, `config` | manifest.toml | manifest (system + ai applets) / first-seed-only (user applets) |
| current `enabled`, current `cron_schedule`, `memory`, `last_slot_at` / `next_due_at`, last_run, runs[], `credential_id` (if fanned out) | SQL | always SQL |

User toggles via the UI (enable/disable, change cron) write SQL only — your manifest is unchanged. User edits to manifest.toml propagate via reconcile but don't blow away user-managed runtime state.

---

## per_credential fan-out

If your applet needs to run **once per connected account** (e.g. one Google Calendar sync per Google credential), declare:

```toml
per_credential = true
source = { id = "google" }
```

Reconcile materializes one `app_applets` row per active credential of that source. Each row gets `credential_id` set; runtime auto-injects the matching credential into `AppletInput.credentials` when the applet fires.

`webhook` triggers **require** `per_credential = true` so bearer auth resolves to an identity.

---

## Naming conventions

- Folder name → applet's `id_prefix` (`applet_<folder>`) by default
- A bare `command` (e.g. `["my_applet"]`) must match a `[[bin]]` in `applets/Cargo.toml` for Rust; a multi-element `command` runs via `PATH`
- A face-only applet sets neither `command` nor `agent`
- `id_prefix` override is rarely needed; only set it if migrating an existing applet

---

## What gets invoked when

| Applet shape | Cron tick | Webhook POST `/webhook/:id` | Manual fire | Detail page |
|---|---|---|---|---|
| has `command` | spawns subprocess | spawns subprocess | spawns subprocess | run log |
| has `agent` | runs the agent loop | same | same | run log |
| face only | skipped (never enqueued) | n/a | skipped | renders the face |

For a subprocess, the trigger payload is delivered as JSON on stdin.

---

## When to choose what

| You want | Pick | Why |
|---|---|---|
| Run every N minutes/hours | `command` + `schedule` | Lowest overhead; fork-per-trigger is fine |
| React to a device webhook | `command` + `triggers = ["webhook"]` + `per_credential` | Standard pattern for iOS/Mac streams |
| Multi-step LLM agent loop | `agent = "..."`, no `command` | Built-in agent runner |
| Dashboard of `data_*` tables | `face/index.html` only | No backend needed; the face queries directly |
| Polyglot script (Python, Node, Bash) | `command = [...]` | Cargo not involved |

---

## Common pitfalls

- **Forgot to `cargo build`** — for Rust function applets, the binary must exist before reconcile. Builds happen at the workspace level: `cargo build --bin <bin-name>`.
- **`webhook` trigger without `per_credential`** — reconcile will refuse the manifest. Webhooks need a credential to authenticate.
- **Face-only applet with `triggers = ["cron"]`** — the scheduler skips anything with no command and no agent, so it ticks into nothing. Set `triggers = []`.
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
│   ├── face/index.html              # optional face (sandboxed iframe)
│   ├── schema/NNNN_*.sql            # optional owned tables, append-only
│   └── ...
└── MANIFEST_SCHEMA.json             # validate your manifest against this
```

That's the whole story.
