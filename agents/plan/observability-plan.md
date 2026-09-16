# One logbook: keys on every line, one door for clients

> Status: **Slices 0 and 1 built on `wave`, unreleased. Slice 2 open.**
> Planned 2026-09-09; trimmed 2026-09-14 to the three slices that pay back on
> the first incident. The general events table and the
> Logbook UI are deferred until a real incident shows they are needed (see
> the end). Delete this file when slice 2 ships.

## The problem, in one paragraph

Answering "why did the box do that at 3am" takes four tools: `journalctl` for
the process, the System view for the machine, the applet's run log for the
job, and `psql` against `app_ai_calls` for the spend. Each is honest. None can
name the others: no span carries a `run_id`, so a failed run cannot find its
own journal lines; the web app's 135 `console.*` calls never leave the
browser, so a phone that fails to pair is invisible to the box; and the one
thing that does leave the box, the crash beacon, never writes the crash
locally and sits under a comment saying the box sends nothing anywhere.

## Goal

1. **Every journal line carries its key.** `run_id`, `chat_id`/`turn_id`,
   `request_id`, `device_id`, stamped by the span you are inside, so one
   `journalctl … -o json | jq 'select(.run_id=="…")'` tells a run's whole
   story.
2. **Clients report through one door.** Web, phone, Mac, desktop post
   `warn`+ to the box as their paired device; the box re-emits into its own
   journal. A client failure becomes a box fact.
3. **The code says what it does.** The crash is recorded locally before it
   is sent, and the opt-out is documented.

**Non-goals.** Central telemetry. A metrics exporter. A log store in
Postgres. A new events table or a Logbook UI (deferred, below).

## What is true today

Read against the code on 2026-09-09.

| Where | What | Why it matters |
|---|---|---|
| `virtues-core/src/main.rs:84`, `applets/src/lib.rs:32`, `crates/virtues-iroh/src/lib.rs:45`, `apps/desktop/src/main.rs:240`, `services/*/src/main.rs` | Five `tracing_subscriber::fmt()` inits, plain text to stderr | Same shape everywhere; one shared init is a refactor |
| every `Cargo.toml` with tracing-subscriber | `features = ["env-filter", "json"]` | JSON output is compiled in and never switched on |
| whole tree | 535 `tracing::*!` sites, **0** `#[instrument]`, no request-id middleware | No line is findable by anything but time and grep |
| `applet_runner/mod.rs:1057-1092` | subprocess stderr: one `tracing::warn` plus a 500-char tail on the run row | Full stderr survives nowhere |
| `cli/report_crash.rs`, `cli/diag.rs` | `ExecStopPost` tails 50 journal lines and POSTs to atlas; default-on; opt-out `VIRTUES_DIAG=off` | **The crash is never written locally.** `VIRTUES_DIAG` is in no doc and no installer output |
| `main.rs:92` | comment: "Virtues collects no central telemetry" | False while the beacon is default-on |
| `apps/web/src` | 135 bare `console.*`, no wrapper, no `onerror`, nothing posted to the box | Client failures are invisible to the box |
| `apps/mac-source/Sources/Core/Uploader.swift:208`, `applets/mac_ingest/main.rs:103` | already ships `collector_health` with every upload; `mac_ingest` parks it on the device row and warns | **This plan said "the box reads it and discards it". Wrong** — it is stored and warned about. What was missing is the *transition*: the warning fires on every ingest for as long as a grant is missing, so a permission lost five minutes ago and one lost a month ago read identically |
| `middleware/rate_limit.rs` | per-IP sliding window, guards `/api/pair/consume` only | Reusable shape for a per-device limit on the door |
| `docs/operate/recovery.md:28` | "Everything logs to the journal. There is no Virtues log file." | The one line of doctrine; this plan extends it |

## Slices

### Slice 0 — tell the truth (no migration) — **BUILT 2026-09-14** (`0b13e5a5`, `a6dcb2ea`)

- `report-crash` emits `box.crashed` with the 50-line tail as a tracing
  event before any send.
- Fix the `main.rs` comment. Document `VIRTUES_DIAG` in recovery.md. Print
  its state in `virtues doctor` and `virtues status`.
- **Gate:** `grep -rn "collects no central" virtues-core/src` is empty;
  `virtues doctor` on dragon prints a diagnostics line.

### Slice 1 — keys on every line — **BUILT 2026-09-15** (`0e09d45b`, plus the request-id middleware carried in `da0d6cc6`)

- `virtues-core/src/observe.rs`: the field-name constants (`kind`,
  `severity`, `source`, `run_id`, `chat_id`, `turn_id`, `request_id`,
  `device_id`) and a shared `init_tracing()` every binary calls: env-filter,
  JSON when stderr is not a TTY, text when it is. Five inits become calls.
- A `from_fn` middleware minting the request id, opening the span, and
  returning `x-request-id`. (Written as `tower_http::TraceLayer` with a
  request-id maker; built as a plain middleware instead, matching the
  `stamp_box_build` layer beside it. TraceLayer would also have emitted its
  own request/response lines on every static asset, which is noise this box
  does not need, and the span is the part we were actually after.)
- THREE spans — HTTP request, applet run, chat turn — plus one keyed event
  on the AI call, which needs no span of its own because it has no children;
  it inherits whichever of the three encloses it. Not the five written here.
  The scheduler tick got nothing: every cron fire reaches `run_applet` →
  `execute_prepared`, which opens the run span, so a tick span would have
  been a second key for the same work and an empty one on the ticks that do
  nothing. The AI-call line is at `debug` because the row in `app_ai_calls`
  is already the durable record; the line exists to be switched on while
  chasing something.
- Applet subprocess stderr re-emitted line by line at `warn` inside the run
  span. The 500-char tail on the run row stays as the UI summary.
- **Gate:** on dragon, `journalctl -u virtues -o json -n 2000 | jq -r
  '.MESSAGE | fromjson | .span.run_id' | sort | uniq -c` shows run ids, and
  a chosen failed run's full stderr is found by that id alone.

### Slice 2 — the client door — **BUILT 2026-09-15**

- `POST /api/events`: paired-device authed, batched body of `{kind,
  severity, message, detail, occurred_at}`. The box stamps
  `source=device:<id>` from the session, never the body, and re-emits each
  as a tracing event. Per-device limit using a keyed variant of the
  sliding window in `rate_limit.rs`; body capped; `detail` capped at 4 KB.
- `apps/web/src/lib/log.ts`: `log.info/warn/error/report`. Writes to the
  console and enqueues `warn`+ for the box; flushes on a timer and on
  `visibilitychange`; drops past a cap when offline; never throws.
  `window.onerror` and `unhandledrejection` feed it. The 135 `console.*`
  calls move onto it mechanically; prefixes become `component`.
- The Mac collector's `collector_health` block produces a
  `collector.permission.lost` / `.granted` line on *change* only, by reading
  the device row before overwriting it. No Swift change. (Trap found while
  building: the wire keys are snake_case, `full_disk_access`, while the Swift
  properties they come from are camelCase. Comparing the camelCase name would
  have compiled, run, and simply never fired.)
- **Gate — partly met.** Verified in a browser against the dev stack: an
  uncaught error is captured, batched and posted with `keepalive`, carrying
  kind, severity, message, stack and `occurred_at`. The server leg is covered
  by unit tests on the shipped `prepare` path, NOT by a live round trip — the
  shared `make dev` server runs a build without the route and restarting it
  would disturb other agents. **Still to do on a real box:** the 200 path and
  the journal line with `source=device:<id>`, and pulling Full Disk Access on
  a Mac to see exactly one `collector.permission.lost`.
- **Found while verifying, and fixed:** with no stand-down, a client posts
  into a 404 every ten seconds forever on any box older than this route —
  which is the normal state after a client release, since phones update
  themselves and boxes do not. A 404 now stops reporting for the session.
- **Found in the post-build audit, and fixed** (`apps/web/src/lib/log.test.ts`
  pins all three):
  1. A full queue sent `MAX_BATCH` events **plus** the drop-notice, and the
     server takes the first `MAX_BATCH` — so the notice, appended last, was
     discarded exactly when there was something to report. A slot is now
     reserved for it.
  2. A `401` retried every ten seconds forever. The airlock and pairing
     screens are unpaired by definition and are where a failure is most
     likely. It now keeps the backlog (pairing can still happen in this
     session) but stops the clock, degrading to one attempt per new event.
  3. The flush timer ran forever once started, waking a phone every ten
     seconds over an empty queue. It now stops when the queue drains and
     re-arms on the next event.
- **Wired 2026-09-15, after the audit found it claimed but absent:** the
  request-id join. The box returned `x-request-id` and nothing on the client
  ever read it, so a client report and the server's account of the same
  failure still could not be put on one key. `ApiError` now carries the exact
  id of the request that failed; the client keeps the last id it saw as a
  fallback for uncaught errors that have no request of their own; and the
  report says which of the two it holds, because a hint that looked like a
  fact would produce a confident join to the wrong request.
- **Fixed 2026-09-15:** applet subprocess logs were double-encoded. Their
  stderr is a pipe, so format auto-detection said "not a terminal" and emitted
  JSON, which the runner then wrapped inside the `message` field of its own
  JSON line. `observe::Format::Text` lets a process whose output is re-emitted
  by another of ours opt out of the guess.
- **Deferred, deliberately:** moving the 135 existing `console.*` calls onto
  the wrapper. Nearly all sit in `catch` blocks, which means the code already
  handled the failure; the errors that break a screen are the uncaught ones,
  and those are covered by the global handlers. The sweep touches ~45 files
  that other agents are actively editing, so the churn would cost more than
  it returns. Do it opportunistically, file by file.

## Decisions

1. **Crash beacon stays default-on** through the `0.1.x` line, made visible
   by slice 0. Flip to opt-in later if wanted; nothing here depends on it.
2. **Clients forward `warn`+ only.** Anything finer is the browser console's
   job.
3. **Settled 2026-09-15: the beacon sends errors and panics only.** The tail
   was 50 unfiltered lines, and a crashing box's last 50 lines are exactly
   where a path, a query, or an error quoting user text is most likely to
   appear — one careless `info!` from making `virtues-api.md`'s "that data
   never leaves your box" false. It now keeps a line only if it is NOT one of
   our JSON records (a panic, a backtrace, an OOM message — plain text,
   because the panic hook never goes through tracing) or IS one at `ERROR`.
   Everything at INFO/WARN/DEBUG is dropped, which is both the bulk of the
   volume and the part that narrates the user's life.

   Note for anyone reaching for the obvious version: `journalctl -p err` does
   NOT work. systemd stamps every line a service writes to stderr as `info`
   absent a `<N>` syslog prefix, which the tracing formatter does not write,
   so filtering by journal priority drops our own ERROR lines. The level lives
   inside the message. Slice 1 is what made this cheap — before the JSON
   switch there was no level to match on.

## Deferred, and what would un-defer it

- **`app_events` table** (auth events plus stall transitions, update
  outcomes, sidecar flaps, crashes, client errors) and a **Logbook** in the
  System view with journal drill-down over a session-authed `journalctl`
  route. Build when someone other than the operator needs to answer "what
  happened" without ssh, and let that incident shape the schema.
- Retiring `app_auth_event` follows from the above.

## Constraints

- Live users, append-only migrations, boxes do not auto-update. These
  slices need no migration.
- Nothing from a real life in `detail`: the `log` wrapper truncates
  messages and never serializes request bodies or page content.
- A failing observability write must never fail the thing observed. Each
  such swallow is commented, per the query-error rule.

## Review register

_Empty until reviewed._
