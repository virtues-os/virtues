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

## Track A — data integrity (no architecture change)

Fixes F1–F5 and F7's reconcile. Self-contained to the iOS queue + `ios_ingest`
+ `action_runner`. Order by impact.

### A1. Deterministic record ids for location & HealthKit  *(linchpin)*

- **Where:** iOS payload structs / `combine()` for `CoreLocationStreamData` and
  `HealthKitStreamData` (+ the HK subtypes).
- **What:** generate a stable `id` per record **on the device** — `UUIDv5` of
  `device_id + timestamp + lat/lon` (location) and `device_id + timestamp +
  metric_type + value` (HK) — and **persist it with the SQLite row** so a retry
  re-sends the *same* id. The box already dedupes on it; no box change needed
  beyond confirming `stream_id_or_new` reads `id`.
- **Why first:** the moment this lands, every retry is harmless — it de-risks
  unbounded retry (A2) and the timeout churn (A4).

### A2. Split transient vs permanent; dead-letter instead of delete

- **Where:** `BatchUploadCoordinator.handleFailedUpload` + `SQLiteManager`.
- **What:** classify `NetworkError`. **Transient** (timeout, no-connection, 5xx,
  429, `notProcessed`, lost-ack) → retry indefinitely with the existing
  backoff+jitter; **do not** count toward a delete cap. **Permanent** (400, 403,
  decode failure) → move to a `dead_letter` status (new) with the reason; surface
  in the UI; never auto-delete un-acked data. Remove the 3-day delete of
  transiently-failed rows.
- **Note:** keep a (large) cap on *attempts-without-progress* only as a
  poison-pill guard → dead-letter, not deletion.

### A3. Advance HealthKit anchor only after durable enqueue

- **Where:** `HealthKitManager` query→enqueue→anchor sequence.
- **What:** write the samples to SQLite first; persist the new anchor only after
  the enqueue is confirmed. On enqueue failure, do not advance the anchor (the
  next query re-emits).

### A4. Reconcile-after-timeout via the runs API

- **Where:** `BatchUploadCoordinator` upload path + existing
  `NetworkManager.fetchActionRuns`.
- **What:** on a tunnel timeout, before counting a failure, query
  `GET /api/devices/applets/:id/runs` and match the recent run
  (`result_summary`/`records_processed`/time) to confirm the batch landed. If it
  did, mark complete (with A1, even a missed reconcile + resend is harmless).
- **Effect:** kills the false "Not reaching box" and the needless resend.

### A5. Server-side: bound and de-wedge execution

- **Where:** `action_runner` + `scheduler::applets`.
- **What:** (a) add a subprocess timeout for `ios_ingest`; (b) add a TTL/watchdog
  so a stale `running` run is reaped without a server restart; (c) consider a
  **warm execution path** for `ios_ingest` (in-process handler or a warm pool /
  persistent PG connection) to shrink the per-call cost that drives the timeout
  race; (d) since ingest is idempotent, consider allowing concurrent ingest runs
  (drop or narrow the per-applet lock for this applet) so sequential stream
  uploads don't 409-cascade.

### A6. `ios_ingest` atomicity + byte-bounded batches

- **What:** wrap a webhook's writes in one transaction so a partial failure
  doesn't leave half a batch (and doesn't trigger a duplicating resend).
  Bound the device's per-stream batch by **bytes** (not just count) so a large
  audio backlog never exceeds the 105 MB body limit (→ 413); apply
  back-pressure to collection if the queue hits its cap rather than rejecting new
  data.

### A7. Low Power Mode

- **What:** allow uploads in LPM at least on Wi-Fi/charging, or surface a clear
  "uploads paused (Low Power Mode)" status so the stall isn't silent.

**Track A acceptance:** a stream that is unreachable for hours, then reconnects,
delivers every record exactly once with no duplicates and no loss; a slow box run
never shows as a client failure; a crashed run never wedges ingestion.

---

## Track B — background reliability (product decision)

Fixes F6. This is the only path to *sanctioned, reliable* background delivery,
but it reverses a deliberate product choice, so it needs sign-off.

### The decision

Re-implement the iOS tunnel as a **WireGuard `NEPacketTunnelProvider`
(WireGuardKit) + VPN On-Demand**, and move uploads to a **background
`URLSession` `uploadTask(fromFile:)`**. The system raises the tunnel on demand
and routes the out-of-process upload through it with the app suspended or killed
— the OS-owned upload daemon and the encryption tunnel finally coexist.

### Why it's compatible with the locked networking doctrine

It is still **direct IPv6 WireGuard with SPKI pinning — no overlay, relay, or
coordinator.** Only the iOS *implementation* changes (system VPN extension vs
in-app userspace library). `networking.md`'s doctrine is unaffected.

### The tension and its mitigation

`NEPacketTunnelProvider` is a *system VPN* (VPN badge, NetworkExtension/Personal
VPN entitlement, separate extension target + App Group), which reverses
BoxTransport's deliberate "the tunnel runs inside the app — it does not take over
your device VPN" stance. **Mitigation: a split-tunnel On-Demand rule that routes
only the box's IP through the tunnel**, leaving all other traffic direct. You get
sanctioned background uploads without becoming a global VPN.

### Complement, not a foundation

Silent APNs background push (`content-available`) can be an **opportunistic drain
accelerator** (kick a drain after buffering) — but it's throttled (~3/hr) and
never guaranteed, so it can't be the primary mechanism. Ties into the noted-but-
not-built APNs wake primitive.

### Cost

Real: extension target, App Group shared storage for the upload queue, the
NE/Personal VPN entitlement, packaging, and reworking `BoxTransport` to hand the
transfer to a background session. Recommend a DTS confirmation on the
extension-loopback caveat before committing.

### Options for sign-off

- **B1 (recommended):** migrate to NEPacketTunnelProvider + split-tunnel
  on-demand + background `uploadTask(fromFile:)`. Sanctioned, reliable, doctrine-
  compatible.
- **B2:** keep in-app WG; lean harder on BGProcessingTask + silent push; accept
  that background delivery remains best-effort and that location-keepalive is a
  standing App-Store risk. Lower cost, does not actually fix F6.
- **B3:** defer Track B; ship Track A only and re-evaluate once we have field
  data on how often delivery actually stalls.

---

## Recommendation

Ship **Track A now** (it fixes everything users actually feel — duplicates,
false failures, silent loss, the lock-wedge), and take **Track B (B1)** as a
separate, scheduled effort after sign-off. Track A does not depend on the Track B
decision, and A1 (deterministic ids) should land first regardless.

## Open questions

1. Track B: B1 vs B2 vs B3?
2. Dead-letter UX — how should permanently-failed records surface to the user
   (and do we offer a manual re-send / export)?
3. Should the confirmed-cursor protocol (F7) be part of Track A, or deferred
   until after the runs-API reconcile proves out?
