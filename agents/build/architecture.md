# Applet architecture

The implementation contract for the applet system: how core loads, reconciles
and runs applets. What an applet *is* and why it has this shape is the record,
[`applets.md`](../record/applets.md). How to write one is
[`applets/AUTHORING.md`](../../applets/AUTHORING.md) (builtin, compiled) and
[`applets/AGENTS.md`](../../applets/AGENTS.md) (chat-authored, declarative).
This page does not repeat either.

## Shape is derived

There is no `runtime` field. `command` makes a subprocess, `agent` an agent
loop (after the subprocess, if both are set), and a `face/index.html` with
neither is a face-only applet that core never invokes. Code that needs the
shape derives it from field presence; nothing may add a field that declares it.

## Field ownership

Manifest fields and `app_applets` columns are disjoint: the manifest owns what
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
  applets under `user/<slug>/`, Git packs imported through
  `POST /api/admin/applets/import-git` under their slug.
- `resolve_applet_dir()` checks state first, so an applet in the state root
  **shadows** a shipped one with the same dir, and deleting it reverts to the
  shipped version.
- Reconcile's system-row GC is guarded on the **shipped** root's template
  count, not the merged catalog's. A shipped root that failed to load plus one
  authored applet is a non-empty catalog, and guarding on that would delete
  every system row.

## Reconcile

`reconcile_templates(db)` is the only writer of manifest → SQL. The parsed
catalog is cached in a `OnceLock<RwLock<…>>`; `reload_and_reconcile()` is the
one way to apply on-disk changes (swap the catalog, then reconcile). Callers:
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
2. Face-only (no command, no agent): skip silently. The scheduler never
   enqueues these either.
3. Webhook: must resolve to a `device_id` or a `credential_id`.
4. `condition` (SQL): a falsy result skips **without** a run row. The
   `message` wake is exempt; `manual` is not.
5. Singleton: a wake during a live run records a `skipped` run.
6. Limits (`applet_runner/limits.rs`), then the run row, then credentials →
   subprocess → agent.

**Subprocess.** `command[0]` without a `/` resolves to a Cargo-built binary
(`VIRTUES_APPLETS_BIN_DIR`, then `/usr/local/libexec/virtues`, then beside the
running core binary in dev), else it is left for `PATH`. Input
(`config`, decrypted `credentials`, `payload`) goes on stdin; the environment
is cleared and rebuilt from `ENV_PASSTHROUGH`, which carries service
endpoints and paths, never secrets. An applet whose folder does not resolve
under the shipped root runs jailed under `systemd-run` (`NoNewPrivileges`,
read-only system, memory and time ceilings); in a Linux release build a
missing `systemd-run` refuses the run rather than running it unjailed. The
jail limits blast radius; it is not an authority boundary, and `build_command`
says why.

**Faces** are served by `server/faces.rs` into a `sandbox="allow-scripts"`
iframe with an opaque origin, and every file carries a CSP `sandbox
allow-scripts`, so a face loaded on its own (a link to it, a typed address)
gets the same opaque origin and never the box's. Svelte components are never
loaded from an applet folder: the app bundle is trusted code and applet
folders are not.

## Why subprocesses

Fork-per-trigger via `tokio::process` costs no dependency and ~50–100 ms per
run. Containers were rejected: Docker on macOS is a multi-gigabyte VM, a cold
container start is slower than a fork, and a registry and image pipeline buy
nothing when the owner is the only one installing extensions. Work that must
outlive a core restart does not belong in an in-process supervisor, which
dies with core.

## Where things live

| What | Where |
|---|---|
| Manifest schema | [`applets/MANIFEST_SCHEMA.json`](../../applets/MANIFEST_SCHEMA.json) |
| Catalog, roots, reconcile | [`virtues-core/src/applet_templates/mod.rs`](../../virtues-core/src/applet_templates/mod.rs) |
| Runner, jail, env | [`virtues-core/src/applet_runner/mod.rs`](../../virtues-core/src/applet_runner/mod.rs) |
| Faces | [`virtues-core/src/server/faces.rs`](../../virtues-core/src/server/faces.rs) |
| Applets UI | [`apps/web/src/lib/components/applets/`](../../apps/web/src/lib/components/applets/) |
