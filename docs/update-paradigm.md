# Box update paradigm

> **BUILT. This is now the design record, not the plan** (reviewed 2026-08-17).
> Every pillar below shipped: release slots with an atomic symlink flip,
> migration lineage preflight under the staged binary, `prepare`/`activate`
> split so installing is a restart rather than a download, `rollback`, and a
> commit-based comparison that made edge-to-edge work. The code is
> `cli/upgrade.rs` and `api/updates.rs`, and both carry the reasoning inline —
> **read those for current behavior.** The sentence this document used to open
> with ("today's `virtues upgrade` implements a naïve subset") was true when
> written and had been false for weeks.
>
> Kept for the three grounded failures below, which are the reason the design
> looks the way it does and are the first thing to re-read before changing it.

How a Virtues box moves between builds — designed after three real failures in one
`virtues upgrade` (2026-07-09): edge-to-edge was impossible, a mainline `--pre` bricked the box
mid-swap over a migration mismatch, and the upgrade managed sidecars the box doesn't have while
ignoring the one it does.

## The three failures (grounded)

1. **Edge-to-edge is a no-op.** Every edge/prerelease build reports the bare `CARGO_PKG_VERSION`
   (`0.1.0`). `upgrade.rs:47` compares `target == current` as semver strings → "already on 0.1.0 —
   nothing to do" — and this early-returns *before* the `--force` check (`:64`), so even `--force`
   can't pull a fresh edge. There is no way to swap one edge build for another via the CLI. (The box
   only got its current edge via the install script, which skips the check.)

2. **Migration divergence bricks mid-swap.** A box that ran a branch/edge build has that branch's
   migrations applied (Dragon: 27–31, incl. branch-only `0028`–`0031`). Mainline `staging.56` lacks
   them, so sqlx's "every applied migration must exist in the binary" check fails
   (`migration 28 … previously applied but is missing`). Critically this runs **after** the binary +
   web + applets were already swapped (`:197`), leaving the box **half-upgraded and down**, with only
   a printed manual-rollback hint — and web/applets are *not* rolled back at all.

3. **Wrong sidecar topology.** `upgrade.rs` hardcodes `virtues-embed` + `virtues-rerank`
   (`:139,:152,:242`). Dragon (Q6A/NPU) runs **`virtues-qnnd`** and no embed/rerank. `service_stop`
   doesn't check unit existence, so it emits "Failed to stop … not loaded" noise **and never restarts
   qnnd** — so a release that changes the qnnd contract silently runs stale.

## The paradigm — five pillars

### 1. Build identity beyond semver
Prerelease/edge builds are indistinguishable by version. Identity must include the **git SHA** (already
embedded — `--version` prints `edge "jangly-cobra" · 2026-07-08 · 13cfd9c`).
- Compare the **target release's commit** vs the running binary's SHA. Different → upgrade; same →
  nothing to do. For stable tags keep semver compare (+ downgrade guard).
- `--force` bypasses the equality short-circuit entirely (move the force check *above* the
  `target == current` return).
- Source of the target SHA: the tarball carries a `BUILD` manifest (`{version, sha, built_at,
  channel}`); the running binary exposes the same. No GitHub API guesswork.

### 2. Migration preflight — never half-swap
Add `virtues migrate --check`: load `_sqlx_migrations`, diff against the binary's embedded set, exit
non-zero with a precise message on divergence (applied-but-missing / checksum drift) — **applying
nothing**.
- Upgrade runs `staged_binary migrate --check` from the staging dir **before any swap**. Fail →
  abort clean, box untouched, with the real reason: *"this build lacks migrations 28–31 that the box
  has applied (branch lineage); refusing. To cross lineages, reset the DB."*
- This turns failure #2 from a mid-swap brick into a pre-swap refusal.

### 3. Atomic release slots + complete rollback
Stop refreshing components in place. Stage a whole release, flip a symlink.
```
/usr/local/share/virtues/releases/<build-id>/   {virtues, llama-server|qnnd, web/, applets/, applets-bin/}
/usr/local/share/virtues/current -> releases/<build-id>       # services reference `current`
```
- Upgrade: stage → preflight (migrate --check + `virtues --version` smoke) → **flip `current`
  atomically** → restart. Any failure before the flip = box untouched; after = flip back.
- Rollback is one symlink flip and rolls **binary + web + applets together** (today web/applets
  aren't rolled back, so a failed upgrade leaves new web on an old binary — exactly what happened).
- Keep last N releases → `virtues rollback` is instant, no re-download.

### 4. Topology-aware sidecars (single source of truth)
The installer writes an install manifest — the one place that knows the box's shape:
```
/usr/local/share/virtues/install.json
  { "profile": "q6a-npu", "sidecars": ["virtues-qnnd"], "models_dir": "…", "web": "…" }
```
Upgrade **reads it** to decide what to stop/restart. Fallback: enumerate `virtues-*.service` via
`systemctl list-unit-files` minus `virtues` itself. Guard every stop/start on unit existence
(`is-enabled`) so absent units are skipped silently, and **restart the sidecars that exist** (qnnd on
Dragon) when the release changes their binary/contract. Kills the "not loaded" noise and the
never-restart-qnnd bug.

### 5. Component-scoped updates
Not every change needs a binary swap + migration + sidecar bounce.
```
virtues upgrade --only web            # refresh just the SvelteKit build (static; zero risk)
virtues upgrade --only web,applets    # web + applet manifests/bins
virtues upgrade                       # full release (binary + migrations + sidecars)
```
- `--only web` is the safe fast path for UI iteration and "I just want to see the new screen" — no
  migration, no binary, no restart (static files; the app's in-process Rust is what serves API).
- On a dev/lab box iterating a branch, this is the day-to-day loop; full upgrades are for releases.

## Migration lineage policy (the root of failure #2)
- **Numbers are stable branch → main.** A branch migration keeps its number/checksum when merged, so a
  box that ran the branch accepts mainline post-merge (checksums match, no "missing"). Never renumber.
- **Edge boxes are branch-tainted** until the branch merges — they ride branch/edge builds, not
  mainline. The preflight (Pillar 2) enforces this with a clear refusal instead of a brick.
- **Crossing lineage is explicit + destructive**: `virtues db reset` / re-provision, for dev boxes
  that accept data loss to jump between unrelated branches.

## Dev-push path (lab box iteration)
Given the lab *is* the box (`ssh <your-box>`), the inner loop should be:
- **UI/applets change** → `virtues upgrade --only web` (or `web,applets`) — seconds, no risk.
- **Rust/migration change** → rebuild `edge` tag → `virtues upgrade` (SHA differs → proceeds; Pillar 1)
  → preflight gates migration safety (Pillar 2) → atomic slot flip (Pillar 3).
- No more `--force` fighting, no more mid-swap bricks, no more manual rollback.

## Bootstrap
These fixes live in the binary, so the *first* improved `virtues` reaches the box the old way (install
script, which skips version-compare, or a manual staged swap). After that, the paradigm is
self-hosting.

## Implementation order
1. **Pillar 1 + `--force` fix** (unblocks edge iteration) — small, in `run()`.
2. **Pillar 2 `migrate --check` + preflight** (stops bricking) — highest safety value.
3. **Pillar 4 topology manifest** (fixes qnnd) — installer writes it, upgrade reads it.
4. **Pillar 5 `--only`** (fast UI pushes) — small, high daily value.
5. **Pillar 3 release slots** (atomic rollback) — largest change; do last, it subsumes the in-place
   refresh + `.bak` dance.
