# Why two bugs were hard to debug — a diagnosability audit (2026-08-27)

Two bugs cost hours that should have cost minutes:

- **A.** Applet subprocesses could not write the lake (EACCES). A fresh box
  failed 26 consecutive `mac_ingest` runs and showed the owner only empty
  data.
- **B.** The desktop app's own IPC commands were ACL-refused. The UI said
  "Can't read this Mac's collector — setting it up again usually clears this."

Both are fixed (`50eeca12`, `f5e8c867`). **This document is about why each one
took so long to find. Neither bug was subtle. In both cases the system had
recorded the cause and did not show it.**

## Summary

**The box recorded the exact cause, verbatim, but every surface an owner looks
at reads a different table.** `app_applet_runs.error` contained the EACCES
message 26 times. Sources reads `data_*` row counts and `app_device.last_seen_at` —
neither of which can see a failed run.

## The five specific reasons

### 1. `last_seen_at` means "authenticated", not "ingested"
`middleware/auth.rs:198` bumps it in the extractor, *before* the handler
decides 200 or 500. A device whose every upload 500s reads **"Last seen 2
minutes ago"** forever, and `SourceDetail.svelte:118` labels it "activity".
This is the single most misleading value in the product: it is the one an
owner checks first, and during incident A it looked healthy and was
completely wrong.

### 2. Four states look the same on screen
Nothing connected / connected but not sending / sending but the box is
rejecting / arriving and stored. Only the first is distinguished. **Thirteen
distinct causes** all render as "Records today 0", from a paused daemon to a
401 self-pause to today's unwritable lake.

The fix was already built and never wired: `stream_health.rs:50-52` computes a
`connected` flag with a comment naming exactly this gap, and
`SourcesOverview.svelte:115` filters on `s.total > 0` instead of
`s.connected`. Likewise `credentials.rs:171-179` computes the
connected→backfilling→live state from run history — and applies it to
credentials only, so Mac and iPhone, the two sources that broke, are the two
that don't get it.

### 3. The error is fetched and then dropped
`SourcesActivity.svelte:110` maps `error: r.error` onto every row. The
`columns` array has no error column. You can filter to "Failed" and get 26 red
rows, each with no reason, on the one page built to answer this question. The
error reached the browser and was never displayed.

The same pattern appears elsewhere: `/api/metrics/activity` returned the last 10 error
messages box-wide and **no client called it** (retired 2026-09-03, along with
the aggregate run panel on the billing page that was its last reader);
`diagnose_box` distinguishes four causes of unreachability and **has zero call
sites**; `shellSupports()`, the documented per-feature degradation gate, **has
zero call sites**.

### 4. Two surfaces on one machine disagreed, and nothing showed which was right
During incident B the tray said **"Collecting" (green)** while the window said
**"Can't read this Mac's collector"** — because the tray calls the Rust
function directly (no ACL) and the window goes through IPC (ACL).
`main.rs:993` compounds it: `get_collector_status` documents at `:414-418`
that a read failure is *not* proof the collector is off, and its only other
caller `.unwrap_or_default()`s exactly that distinction away.

### 5. The release build users run gives the least detailed errors
In a debug build, Tauri's ACL rejection names the missing capability
(`webview/mod.rs:1828-1846`). In a signed release it is the terse "Command X
not allowed by ACL" (`:1850`) — and the release has no devtools (no `devtools`
feature; the inspector is `#[cfg(debug_assertions)]`). So the message that
could have solved it in seconds exists only in debug builds, where the bug did
not occur.

The suggested fix could not work either: "Set up this Mac" calls
`install_collector`, which sits in the *same permissions array* as the
`get_collector_status` that had just been refused. The user was sent into a
loop with no exit.

**The dev loop also makes the broken state the one least often tested.**
`make mac-dev` requires `PROFILE=`, which means a fresh unpaired store — and an
unpaired app never reaches `WebviewUrl::External` at all. The remote origin,
where this whole class of bug occurs, is the state you almost never run locally.

## What logs exist, and who can reach them

| Component | Lands | Reachable without ssh? |
|---|---|---|
| Box server | journald (no file) | **No** — only by typing `journalctl` into Developer→Terminal |
| Applet runs | Postgres, 4 KB error + 500-char stderr tail | **Yes**, per applet at `/applet/:id` — a page with no inbound link from any failure surface |
| Desktop shell | 16 `eprintln!` → macOS unified log | **No** — no file, no plugin, never shipped to the box |
| Collector | `~/.virtues/logs/*.log`, **no rotation, no cap** | **No** — no `logs` subcommand, no bridge command |
| iOS | `NSLog` + a MetricKit table with **no reader** | **No** |

`virtues doctor` would have caught neither bug: it never resolves or
write-probes a path, never prints an env value, and never looks at applet
health. It is also the wrong process to ask — incident A was a *divergence
between two processes' resolution of the same path*, and doctor only inspects
its own.

## The cheap fixes, ranked by minutes saved per line changed

1. **Add an `error` column to `SourcesActivity.svelte:115`.** ~4 lines. The
   value is already on the row. That alone would have made incident A take seconds.
2. **Give devices the `sync_state` credentials already have** — join
   `app_applet_runs` in `api/devices.rs`, reuse `sync_state_for`, mark
   `broken: true` when `total_runs > 0 && success_runs == 0`. ~30 lines, and
   the missing data shows as a red row naming the unwritable lake.
3. **Show `last_ingest_at` beside `last_seen_at`**: "Last contact 2m ago ·
   last stored 6 days ago." That sentence shows the difference between (b) and (c).
4. **`GET /api/system/logs?unit=…&lines=N`** behind auth, allowlisted units.
   Everything needed exists (the sudo grant, the `journalctl` invocation in
   `report_crash.rs`). This would have made investigation A much shorter.
5. **Print resolved paths in `doctor` and telemetry** — `lake_root()`,
   state root, applets dir — each with a real write-and-unlink probe, and note
   whether the value came from env or fallback.
6. **Fix the two false sentences in `ThisMacView.svelte:209-211`** and promote
   the raw error from `text-xs text-foreground-subtle` to body weight.
7. **Filter on `s.connected`, not `s.total > 0`** (`SourcesOverview.svelte:115`).
   2 lines; the flag is already on the wire.

One caveat: `AppletsPanel.svelte:57-59` already argues that filing
non-breakage beside breakage teaches people to ignore the strip. Any widening
of "needs attention" must exclude sources the owner never connected, or it
raises false alarms from the first day.

## Security finding, unrelated to either bug but found on the way

**The box-served SPA is a remote origin holding `shell:allow-execute` and
`shell:allow-spawn` for `osascript` with `"args": true`**
(`capabilities/default.json:42-83`, applied to `http://localhost:*` via
`remote.urls`). No JavaScript in this repo calls them; the Rust-side
`app.shell().command(...)` paths bypass the ACL entirely. So our own code never
uses the grant, but whatever serves the SPA can: a compromised box can run
arbitrary code on the paired Mac. The `binaries/virtues-client` entries beside
it are also stale — that sidecar stopped shipping when reach moved in-process.
Deleting both is a few lines and costs nothing.

Also latent: **Android registers five app commands and grants none**
(`capabilities/android.json`), and `src-tauri/build.rs` only checks the
desktop and mobile capability files — so the guard that would catch it does
not look there.
