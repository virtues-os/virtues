# Authoring an Applet

This is the guide for **builtin applets — the ones that ship with the box and run compiled code.** If you are writing an applet from chat, or anything declarative (a prompt, a schedule, a view), [`AGENTS.md`](./AGENTS.md) is the contract you want. The format and the reasons for it are in [agents/plan/applet-shape.md](../agents/plan/applet-shape.md).

> **The whole story in one sentence.** An applet is a folder at `applets/<name>/` with a `SKILL.md` and, optionally, a view, tables or scripts. Drop the folder, hit `POST /api/admin/reconcile`, and the applet is live.

---

## There is no runtime field

An applet's shape is **derived from which keys you set**, never declared.

| You set | What it is |
|---|---|
| `metadata.run` naming a binary or a script | code: forked per trigger, stdin/stdout JSON |
| `metadata.run: sql …` | read-only SQL before the prompt; no rows skips the run |
| a body below the frontmatter | the prompt: an LLM agent loop, after `run` if both are set |
| a `view.html` and none of the above | a view — never invoked server-side |

`SKILL.md` follows the [Agent Skills](https://agentskills.io/specification) spec: `name` (the folder's name, lowercase words joined by `-`) and `description` are required, and the Virtues keys live under `metadata` as strings. The reader (`virtues-core/src/applet_templates/skill.rs`) refuses a file with a sentence for each problem.

---

## Quick start

### A subprocess applet (Rust)

```
applets/
├── my-applet/
│   └── SKILL.md
└── my_applet/
    └── main.rs
```

The applet folder holds the applet; the Rust source lives beside it in a folder `applets/Cargo.toml` names, and has no applet file of its own.

`applets/my-applet/SKILL.md`:

```markdown
---
name: my-applet
description: What this does, in one sentence.
metadata:
  invoke: none
  trigger: cron 0 */15 * * * *
  run: my_applet
---
```

`run: my_applet` is a bare name, so it resolves to the Cargo-built binary. Only a shipped applet may run a binary by name.

`applets/my_applet/main.rs`:

```rust
use anyhow::Result;
use virtues_helpers::{connect_from_env, output, read_input};

#[tokio::main]
async fn main() -> Result<()> {
    let input = read_input()?;
    let _db = connect_from_env().await?;

    // Do work — read/write data_* tables, call APIs, run an agent.
    // input.config holds the applet's config, its state laid under it.
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

### A script applet (Python / shell)

The contract is **stdin JSON in, stdout JSON out, exit 0 on success**. Put the script under `scripts/`:

```markdown
---
name: my-python-applet
description: What this does, in one sentence.
metadata:
  invoke: none
  trigger: every 30 minutes
  run: scripts/main.py
---
```

`scripts/main.py`:

```python
#!/usr/bin/env python3
import json, sys

inp = json.load(sys.stdin)
# inp = { "config": {...}, "credentials": {...} | None, "payload": ... | None }

# do the work...
result = "synced 42 records"

print(json.dumps({"result": result, "config": inp["config"]}))
```

A `.py` runs with `python3`, a `.sh` with `sh`, anything else directly. Reconcile picks it up after the folder lands.

That example ships with the box, so it runs directly as the box user. Code
that did not ship (an import, or an applet in the state root) runs in a
sandbox with no network, as a throwaway user, and sees:

| Path or variable | What it is |
|---|---|
| `/skill` | its folder, read-only; the working directory |
| `/lake`, `STORAGE_PATH` | its own lake folder, writable; indexed after each run, refused while over 2 GB |
| `/data/lake` | the whole lake, read-only, only when `allowed-tools` lists `lake` |
| `DATABASE_URL` | the Postgres socket as `virtues_applet_writer`, which writes applet schemas only |
| `$CREDENTIALS_DIRECTORY/credentials.json` | its credentials; `inp["credentials"]` is `None` |
| `HTTPS_PROXY`, `HTTP_PROXY` | set only when the run may reach hosts: the ones its `allowed-tools` asks for with `net(host)` that the owner allowed on the applet's page. Every request is recorded on the run |

### A view-only applet

A view is `view.html`, one file of at most 48 KB, rendered in a sandboxed iframe. There is no Svelte option: an applet folder must not mount components inside the app bundle, which is the whole point of the iframe's opaque origin.

```
applets/
└── my-dashboard/
    ├── SKILL.md
    └── view.html
```

```markdown
---
name: my-dashboard
description: What this shows you, in one sentence.
---
```

Inside `view.html`, link `virtues.css` for the box's theme and use `await virtues.query(sql)` for read-only access to your data. See `applets/dot-cloud/view.html` for a working one. A folder an import brings with a multi-file `face/` keeps serving it from there.

---

## Reconcile

Anytime you create, edit, or delete a `SKILL.md` on disk, tell core to pick up the change:

```bash
curl -X POST http://localhost:8000/api/admin/reconcile
```

Code changes (editing `main.rs`, etc.) require a `cargo build` first if Rust.

A `manifest.toml` folder in the state root (an import, a restore from an older box) is converted to `SKILL.md` before the catalog reads it (`applet_templates::convert`); one that cannot convert is logged and not loaded.

---

## Field ownership

`SKILL.md` is **declarative** — what the applet *is*. SQL (`app_applets`) holds **runtime state** — what it's doing right now:

| Field | Lives in | Wins on conflict |
|---|---|---|
| `description`, `run`, `trigger`, `invoke`, `source`, `limit`, `model`, `enabled` (the default), the prompt | SKILL.md | the file (shipped and assistant-made applets) / first-seed-only (your own and imports) |
| current `enabled`, `memory`, `state`, `last_slot_at` / `next_due_at`, runs, `credential_id` (if fanned out), the hosts the owner allowed | SQL | always SQL |

An edit to an assistant-made applet from the app writes its `SKILL.md`, so reconcile reads the edit back.

---

## per_credential fan-out

If your applet needs to run **once per connected account** (e.g. one Google Calendar sync per Google credential), name the source:

```markdown
metadata:
  source: google
```

Reconcile materializes one `app_applets` row per active credential of that source. Each row gets `credential_id` set; runtime auto-injects the matching credential into `AppletInput.credentials` when the applet fires.

Only an applet that ships with the box may name a `source`, because its runs hold the account's decrypted sign-in. An applet in the state root that names one does not load.

A `webhook` trigger **requires** a `source`, so the post resolves to an identity.

---

## Naming conventions

- The folder name is the applet's `name`; its id is `applet_` plus the name with `-` as `_`.
- A bare `run` (e.g. `my_applet`) must match a `[[bin]]` in `applets/Cargo.toml`.
- A view-only applet sets neither `run` nor a prompt.
- `metadata.writes` names the data streams the code writes, for the streams page.

---

## What gets invoked when

| Applet shape | Schedule | Webhook POST `/webhook/:id` | Run now | Detail page |
|---|---|---|---|---|
| has `run` code | spawns subprocess | spawns subprocess | spawns subprocess | run log |
| has `run` SQL | runs it; no rows skips the run | same | same | run log |
| has a prompt | runs the agent loop | same | same | run log |
| view only | never scheduled | n/a | n/a | renders the view |

For a subprocess, the trigger payload is delivered as JSON on stdin.

---

## When to choose what

| You want | Pick | Why |
|---|---|---|
| Run every N minutes/hours | `run` + `trigger: every …` | Lowest overhead; fork-per-trigger is fine |
| React to a device webhook | `run` + `trigger: webhook` + `source` | Standard pattern for iOS/Mac streams |
| A prompt only when something is true | `run: sql …` + a prompt | No model call on the days it is false |
| Multi-step LLM agent loop | a prompt, no `run` | Built-in agent runner |
| Dashboard of `data_*` tables | `view.html` only | No backend needed; the view queries directly |

---

## Common pitfalls

- **Forgot to `cargo build`** — for Rust applets, the binary must exist before it runs. Builds happen at the workspace level: `cargo build --bin <bin-name>`.
- **`webhook` trigger without `source`** — the reader refuses it. Webhooks need a credential to authenticate.
- **A prompt applet missing `invoke: none`** — without it, an applet with a prompt is offered in the `/` menu and to the assistant.
- **Live editing `main.rs`** — reconcile doesn't trigger `cargo build`. You still need to rebuild manually for Rust changes.

---

## Where things live

```
applets/
├── sources.toml                     # [[source]] catalog (auth providers)
├── Cargo.toml                       # [[bin]] entries for Rust code
├── <name>/                          # the applet
│   ├── SKILL.md                     # frontmatter and prompt
│   ├── view.html                    # optional view (sandboxed iframe)
│   ├── tables/NNNN_*.sql            # optional owned tables, append-only
│   └── scripts/                     # optional code
└── <name_with_underscores>/         # Rust source for a shipped binary
    ├── main.rs
    └── ...
```

That's the whole story.
