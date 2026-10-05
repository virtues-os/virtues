# Data durability — reliable collection & delivery (RFC)

> **Status:** Draft RFC for review (2026-06-25). Captures the findings of a
> three-pass audit of the iOS → box ingestion path and the downstream ELT, and
> proposes a fix split into two tracks. Track A (data integrity) is ready to
> implement; Track B (background reliability) is a product decision that needs
> sign-off before work begins.

## The promise we're failing

The iOS app's stated north star is **"reliable raw data collection — zero
silent data loss."** Today the delivery path quietly violates that in three
ways, and a fourth (background reliability) is an architectural dead-end, not a
bug. None of this is visible in the box's run log, which is why it's been
confusing: **the box logs runs as `success` while the app shows "Not reaching
box."** That contradiction is real and explained below.

The reassuring part: the repo already contains the *correct* pattern. Our
cron-pull sync applets (`strava_activities_sync`, `plaid_*`, `google_*`,
`notion_pages_sync`) use deterministic `UUIDv5` ids + a cursor persisted in
`app_actions.config` that advances only after a successful write. That is
exactly the at-least-once + idempotent-receiver + confirmed-cursor design the
literature prescribes. **The iOS push path is the outlier; the fix is largely to
make it conform to a pattern we already trust.**

---

## Findings

### F1 — Transient failures strand, then delete, un-acked data (silent loss)

Any failed exchange — including a transient timeout or a lost ack — calls
`incrementRetry` (`SQLiteManager.swift`), which sets `status='failed'`,
`attempts+1`. After **5 attempts** the row is never dequeued again
(`dequeueNext`: `attempts < 5`) and is **deleted 3 days later**
(cleanup: `failed AND attempts>=5 AND created_at < 3d`). Transient and permanent
failures share one counter, so "wifi was flaky for an hour" is treated like a
hard `400` reject. `forceUpload` ("Send Now") clears the circuit breaker but
**does not reset `upload_attempts`**, so it cannot rescue stranded rows; worse,
its 0.1s loop can burn all 5 attempts in ~1 second.

**This is silent permanent loss of data that was never actually rejected.**

### F2 — Location & HealthKit duplicate on every retry (corruption)

The box dedupes via a `source_stream_id UNIQUE` column, but
`stream_id_or_new` (`crates/virtues-helpers/src/ios.rs`) only dedupes if the
record carries a stable `id` — otherwise it falls back to `Uuid::new_v4()`,
random per send.

| Stream | stable id? | retry-safe? |
|---|---|---|
| Audio | ✅ chunk `id` | idempotent |
| EventKit | ✅ `eventIdentifier` | idempotent |
| FinanceKit | ✅ UUIDv5 of Apple id | idempotent |
| Contacts | ✅ entity id from email | idempotent |
| **Location** | ❌ no `id` → `Uuid::new_v4()` | **duplicates** |
| **HealthKit** | ❌ no `id` → `Uuid::new_v4()` | **duplicates** |

The two **highest-volume** streams are the two that duplicate. Every lost-ack
resend writes fresh rows — which is why the box log shows `locations: 17/17`,
`18/18`, `19/19` repeatedly (real new inserts, not re-confirmations). Duplicate
raw rows then flow into downstream `SUM()`/`COUNT()` aggregations
(`day_summary_eod`), inflating derived metrics.

### F3 — HealthKit anchor advances before the data is durable (silent loss at source)

`HKAnchoredObjectQuery` anchors are saved to `UserDefaults` immediately after the
query, **before** the samples are confirmed in SQLite
(`HealthKitManager.swift`). If the enqueue fails, the samples are gone forever —
the anchor won't re-emit them. This is loss *before* the upload path even sees
the data.

### F4 — "Box success, app failure" is a client-timeout-vs-unbounded-server race

Root cause, confirmed end to end:

- Every webhook spawns a **fresh OS subprocess** (`action_runner/mod.rs`) that
  opens a **cold Postgres connection** (`ios_ingest/main.rs`), with **no
  server-side timeout** — the run stays `running` as long as it takes.
- The client gives up at **30s** (tunnel `READ_IDLE_TIMEOUT`,
  `crates/virtues-tunnel/src/tunnel.rs`) / 60s exchange
  (`BoxTransport.tunnelExchangeTimeout`). A slow run (cold spawn + cold PG +
  contention, or a large batch) crosses 30s → the device throws
  `TunnelTimeoutError` while the box runs to completion and logs `success`.
- **Cascade:** `ios_ingest` is one applet guarded by a per-applet `running`
  lock. After a timeout, the device's next sequential stream POST gets a
  **409 skip** (prior run still active). And a hung subprocess **wedges the lock,
  which is cleared only on server restart** (`scheduler/applets.rs`
  `cleanup_stale_runs`) — so one stuck run can stall *all* of that device's
  ingestion until the box restarts. Latent availability landmine.

The webhook response itself is well-formed (axum sets `Content-Length`;
`complete_run` is awaited before 200), so this is purely a latency race, not a
framing bug.

### F5 — `ios_ingest` is not atomic

Writes are stream-by-stream, batch-by-batch with independent commits
(`ios_ingest/main.rs`, each `flush_*` does its own `execute`). A mid-batch error
returns 500; the device retries the whole batch; dedup absorbs the stable-id
streams but **re-duplicates location/healthkit** (compounds F2).

### F6 — Background delivery is architecturally capped

The in-app **userspace WireGuard tunnel cannot carry OS background uploads** —
iOS background transfers run in `nsurlsessiond`, out-of-process, which never sees
the in-app socket (`BoxTransport` uses a foreground `URLSessionConfiguration.default`).
So the app relies on **continuous background location as a keepalive**, which is:

- **App-Store-risky** — Apple's energy guidance is explicitly against using the
  `location` background mode purely to stay awake; and
- **unreliable** — in airplane mode or when stationary there are no location
  callbacks → the process suspends → the `DispatchSourceTimer` upload cycle
  stalls → no drain until something else wakes the app.

Related stall: **Low Power Mode skips uploads entirely** (`BatchUploadCoordinator`),
so a user in LPM for days keeps collecting but never delivers (data survives as
`pending`, but it's a silent stall that can eventually hit the 500 MB queue cap).

### F7 — Unused plumbing for the right design already exists

The payload's `checkpoint` field, the `elt_stream_checkpoints` table, and a
device-facing runs API (`GET /api/devices/applets/:id/runs`, returning
`status`, `records_processed`, `result_summary`) plus `/api/credentials`
`sync_state` are all present and unused/under-used. Reconcile-after-timeout and a
confirmed-cursor protocol can be built on what's already there.

---

## Design principles (from the literature, mapped to us)

1. **At-least-once delivery + idempotent receiver = effectively once.** Never
   rely on transport "exactly once."
2. **Idempotency key = a stable id generated once and persisted *with* the
   record**, reused on every retry (not regenerated per send).
3. **Never drop a transient failure or a poison message** — retry transient with
   backoff+jitter; move permanent/exhausted to a **dead-letter** state with the
   reason, for inspection. Never silently delete un-acked data.
4. **Advance the confirmed cursor only on durable server ack.**
5. **Bound local storage with back-pressure, not eviction** — un-acked data is
   the only copy.
6. **Atomic write per request.**
7. **Dedup-safe aggregations** (recompute-and-overwrite, never incremental
   counters).
8. **Reliable background transfer = OS-owned out-of-process upload** through a
   system-routed tunnel.

---

## What became of the two tracks (2026-09-29)

The native iOS app this audit read was deleted; collection moved to the Tauri
app with one Rust outbox shared by every collector
(`crates/virtues-reach-client/src/outbox.rs`, from bd54da61). That rewrite, not
a patch series, is where Track A landed:

- **A1** deterministic ids — records without a natural id get `UUIDv5(device +
  stream + canonical record)` on the device, stable across retries.
- **A2** no delete of un-acked data — a row is deleted only on a durable ack;
  every failure is a backoff (30s → 5 min), never an age-out. The one exception
  is a row whose payload no longer parses.
- **A3** the HealthKit anchor is held when enqueue fails
  (`plugins/health/ios/Sources/HealthKit.swift`).
- **A5** subprocess wall-clock timeout plus a stale-`running` TTL
  (`RUN_STALE_TTL_SECS`, `scheduler/applets.rs`).
- **A6** batches are bounded by bytes as well as rows; the microphone stream
  gets back-pressure (enqueue refused past 1 GiB, producer keeps its file)
  rather than eviction. `ios_ingest` is still not one transaction, which the
  deterministic ids make harmless: a resend of a half-written batch dedupes.
- **A7** Low Power Mode throttles the drain (constrained mode) rather than
  stopping it.

**Track B is moot.** WireGuard is gone; the phone reaches the box over iroh, and
background delivery rides location wakes with endpoint parking. An out-of-band
wake for a wedged socket is part of
[`../plan/reminders-plan.md`](../plan/reminders-plan.md).

What is still open — permanent vs transient failures, and one poison record
holding its batch — is [`../plan/ios-durability-plan.md`](../plan/ios-durability-plan.md).
