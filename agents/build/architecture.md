# Applet architecture

The implementation contract for the applet system: how core loads, reconciles
and runs applets. What an applet *is* and why it has this shape is in the design record,
[`applets.md`](../record/applets.md). How to write one is
[`applets/AUTHORING.md`](../../applets/AUTHORING.md) (builtin, compiled) and
[`applets/AGENTS.md`](../../applets/AGENTS.md) (chat-authored, declarative).
This page does not repeat either.

## An applet's shape is derived from its fields

An applet is a folder with a `SKILL.md` (`applet_templates/skill.rs` reads
it; a `manifest.toml` in the state root is converted first by
`applet_templates/convert.rs`). There is no `runtime` field. `run` naming code
makes a subprocess, `run: sql …` a read-only query whose empty result skips
the run, the body an agent loop (after `run`, if both are set), and a
`view.html` with none of these is a view core never invokes. Code that needs
the shape derives it from field presence; nothing may add a field that
declares it.

## Field ownership

`SKILL.md` keys and `app_applets` columns are disjoint: the file owns what
the applet *is*, SQL owns what state it is *in* (`enabled`, the current
schedule, `memory`, runs, a fanned-out `credential_id`). The per-field table is
in AUTHORING.md's "Field ownership" and is kept there. The contract here is the
direction: **reconcile writes folder → SQL, never the reverse**, and a UI
toggle writes SQL only. A new field is assigned to exactly one side.

## Two applet roots

| root | path on a box | env override | lifecycle |
|---|---|---|---|
| **shipped** | `/usr/local/share/virtues/applets` | `VIRTUES_APPLETS_DIR` | package data: root-owned, replaced wholesale each release |
| **state** | `/var/lib/virtues/applets` | `VIRTUES_APPLET_STATE_DIR` | per-box state: service-owned, never touched by the installer |

In a dev checkout they are `applets/` and the gitignored `.applet-state/`
(`shipped_root()` / `state_root()` in `applet_templates/mod.rs`).

- Everything core writes at runtime goes to the state root: chat-authored
  applets under `user/<name>/`, Git packs imported through
  `POST /api/admin/applets/import-git` under their slug.
- `resolve_applet_dir()` checks state first, so an applet in the state root
  **shadows** a shipped one with the same dir, and deleting it reverts to the
  shipped version.
- Reconcile's system-row GC is guarded on the **shipped** root's template
  count, not the merged catalog's. A shipped root that failed to load plus one
  authored applet is a non-empty catalog, and guarding on that would delete
  every system row.

## Reconcile

`reconcile_templates(db)` is the only writer of folder → SQL. The parsed
catalog is cached in a `OnceLock<RwLock<…>>`; `reload_and_reconcile()` is the
one way to apply on-disk changes (convert any `manifest.toml` in the state
root, swap the catalog, then reconcile). Callers:
boot (`server/mod.rs`, after migrations), `POST /api/admin/reconcile`, Git
import, and credential or device pairing changes (`api/source_auth.rs`,
`api/pair.rs`), which change per-credential fan-out.

Reconcile is global and not transactional, so it serializes on
`reconcile_lock()` internally; every caller is covered, not just the wrapper.
It must be idempotent: back-to-back runs produce no diff
(`reconcile_is_idempotent` test).

## Dispatch

`applet_runner::run_applet` runs these gates in order, and an early exit never
reaches the heavy phase:

1. Fetch the row; reject a trigger the applet does not declare.
2. View-only (no `run`, no prompt): skip silently. The scheduler never
   enqueues these either.
3. Webhook: must resolve to a `device_id` or a `credential_id`.
4. `condition` (SQL): a falsy result skips **without** a run row. The
   `message` wake is exempt; `manual` is not.
5. Singleton: a wake during a live run records a `skipped` run.
6. Limits (`applet_runner/limits.rs`), then the run row, then credentials →
   `run` SQL (no rows records a `skipped` run and calls no model) →
   subprocess → agent.

**Subprocess.** `command[0]` without a `/` resolves to a Cargo-built binary
(`VIRTUES_APPLETS_BIN_DIR`, then `/usr/local/libexec/virtues`, then beside the
running core binary in dev), else it is left for `PATH`. Input
(`config`, decrypted `credentials`, `payload`) goes on stdin; the environment
is cleared and rebuilt from `ENV_PASSTHROUGH`, which carries service
endpoints and paths, never secrets. That is for shipped code, which runs
directly as the box user.

An applet whose folder does not resolve under the shipped root runs in the
sandbox (`sandbox`, `applet_runner/sandboxed.rs`): a transient `systemd-run`
unit with a throwaway user, no network and the box's data hidden. It sees
its folder read-only at `/skill` (its working directory), its own lake
folder at `/lake` (indexed into `lake_objects` after each run, and refused
while over 2 GB), the whole lake read-only at `/data/lake` only when
`allowed-tools` lists `lake`, and the Postgres socket with `DATABASE_URL`
logged in as `virtues_applet_writer`. Its credentials are a file,
`$CREDENTIALS_DIRECTORY/credentials.json`, and stdin carries `"credentials":
null`. It has no network unless its `allowed-tools` asks for hosts with
`net(host)` and the owner allowed them on the applet's page
(`app_applets.net_hosts`); then the run gets an egress proxy (`egress`) that
reaches only those hosts, and each request is recorded in
`app_applet_run_requests`. In a Linux release build a missing `systemd-run`
refuses the run rather than running it unsandboxed.

**Faces** are served by `server/faces.rs` into a `sandbox="allow-scripts"`
iframe with an opaque origin, and every file carries a CSP `sandbox
allow-scripts`, so a face loaded on its own (a link to it, a typed address)
gets the same opaque origin and never the box's. Svelte components are never
loaded from an applet folder: the app bundle is trusted code and applet
folders are not.

## Why subprocesses

Fork-per-trigger via `tokio::process` costs no dependency and ~50–100 ms per
run. Containers were rejected: Docker on macOS is a multi-gigabyte VM, a cold
container start is slower than a fork, and a registry and image pipeline add
nothing when the owner is the only one installing extensions. Work that must
outlive a core restart does not belong in an in-process supervisor, which
dies with core.

## Where things live

| What | Where |
|---|---|
| The format, and converting to it | [`virtues-core/src/applet_templates/skill.rs`](../../virtues-core/src/applet_templates/skill.rs), [`convert.rs`](../../virtues-core/src/applet_templates/convert.rs) |
| Catalog, roots, reconcile | [`virtues-core/src/applet_templates/mod.rs`](../../virtues-core/src/applet_templates/mod.rs) |
| Runner, sandbox, env | [`virtues-core/src/applet_runner/mod.rs`](../../virtues-core/src/applet_runner/mod.rs) |
| Faces | [`virtues-core/src/server/faces.rs`](../../virtues-core/src/server/faces.rs) |
| Applets UI | [`apps/web/src/lib/components/applets/`](../../apps/web/src/lib/components/applets/) |
