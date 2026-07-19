# iOS App — Full Audit & Overhaul Plan

_Audit date: 2026-07-07. Covers delivery/durability, transport, threading,
information architecture, onboarding, copy/narrative, and feature scope._

> Many line-level findings below came from finder passes and are marked
> **(confirm)** — verify against the code before editing; some will be wrong.

---

## The diagnosis (what's actually going on)

The app is **three geological layers** that don't agree:

1. **v0** — typed server URL + bearer token + WireGuard + 5-min `URLSession` uploads.
2. **"Jan 2025 refactor"** — added DI, `ReliableTimer`, `HealthCheckCoordinator`,
   and a `CLAUDE.md` that **oversells** them ("zero data loss / zero races / 100%
   background reliability / 5-min sync"). Those claims are **not true**.
3. **iroh transport** — bolted underneath this session.

On top of that, the app **drifted from its one stated job** ("reliable raw data
collection to a self-hosted box") into a **collector + dashboard + diagnostics
hub**, wrapped in copy that **overpromises** (AI / transcribe / "your AI is
learning") and describes a model (server URL / bearer / WireGuard) that no longer
exists.

**Net:** it's not broken (data flows), it's **fragile in the one place it can't
afford to be** (getting data off the phone durably), and **unfocused** everywhere
else. That combination is what reads as "janky."

---

## P0 — Fix the one job: durable delivery (data-loss)

The app's entire purpose. _Verified 2026-07-07._

- **[CONFIRMED — data loss] Stranded `uploading` rows.** `resetStaleUploads`
  (`SQLiteManager.swift:136`) only resets rows with `last_attempt_date < 10 min ago
  OR NULL`; a crash whose upload started <10 min ago leaves the row `uploading`,
  and `dequeueNext` (`:409`) only selects `('pending','failed')` → the row is
  excluded **forever**. → On launch, reset ANY `uploading` row (or scope by a
  per-launch run-id), not by a wall-clock window.
- **[REFUTED after box check] Location/Audio "random UUID → dup" & "re-chunk → dup."**
  The box ingest is idempotent — every stream writes `ON CONFLICT (source_stream_id)
  DO NOTHING` (`actions/ios_ingest/*.rs`, `crates/virtues-helpers/src/dedup.rs`) —
  and the iOS record `id` is generated **once at collection and persisted in the
  blob** (JSON decode preserves it; the random-UUID constructor only runs at collect
  time), so it's stable across retries and re-chunks. No client change needed; do
  NOT touch the ids.
- **[CONFIRMED — but it's a decision + a lie in the UI] Cadence mismatch.** Upload
  timer is **900s / 15 min** (`BatchUploadCoordinator.swift:35`); HealthKit collects
  every 300s; the UI shows a hardcoded **"Auto Sync: Every 5 minutes"**
  (`SettingsView.swift:195`) and the doc claims 5-min. Pick the real cadence; make
  code, UI, and doc agree.
- **[REFUTED] HealthKit anchor-before-save.** The code stages anchors in
  `pendingAnchors` and only `commitAnchors()` **after** `saveMetricsToQueue` returns
  true (`HealthKitManager.swift` ~625/664). So the anchor is durable *relative to the
  SQLite queue* — which is the design's durability boundary. The only residual
  HealthKit-gap risk is downstream: a queued row later lost via the stranded-`uploading`
  bug above. **Fixing the stranded-row bug closes this too — no separate anchor fix.**

**Acceptance:** kill app mid-upload, airplane-mode mid-sync, 7-day backfill →
no row lost, no duplicate, nothing stuck in `uploading`.

---

## P1 — Stop the crashes: threading correctness

_Verified 2026-07-07 — the "zero races" claim is false, but 2 of 5 finder claims
were themselves wrong (verified safe). Real ones:_

- **[CONFIRMED — HIGH] `stopRecording()` doesn't await the in-flight chunk-finalize
  `Task`** (`AudioManager` ~285 vs the `Task {}` at ~449); an immediate
  `startRecording()` can open the same file path while the finalize task is still
  writing/deleting it → file corruption. → await/serialize finalize before restart.
- **[CONFIRMED] Health-check auto-restart has no backoff/cap** — `HealthCheckCoordinator`
  restarts unhealthy managers every 30s forever (Audio/HealthKit have no circuit
  breaker, unlike uploads). Non-recoverable failure = permanent thrash/drain. → add
  backoff + cap.
- **[PARTIAL — LOW] BGTask completion race** — `setTaskCompleted()` runs in a separate
  `Task` awaiting the upload; a mid-await expiration is theoretically missable but
  Swift's cancellation returns `.failure`, so it almost always completes. Tidy the
  structure; low priority.
- **[REFUTED] Off-main `@Published`** — `AudioManager.updateDbLevel` wraps the mutation
  in `DispatchQueue.main.async`. Safe.
- **[REFUTED] HealthKit anchor `Dictionary` race** — an `NSLock` gate (`collectionGate`)
  guards both start and the commit phase; overlapping runs are skipped. Safe.

---

## P2 — Transport hardening (mostly in flight)

- **GSO EIO** — FIXED for the box (`.56`, vendored `noq-udp` GSO off). Note: the patch
  is **Linux-only**; the iOS send path is untouched (revisit if the phone side ever EIOs).
- **Dead direct-addr candidate** — the reach ticket derives `<endpoint host>:51820`
  from the (possibly Tailscale) server URL; with that path dead it's churn
  (`LastOpenPath`). → drop the derived candidate, or prune unreachable candidates;
  don't derive reach from the cosmetic endpoint URL.
- **Timeout mid-send tears the stream; retry resends same body** — mitigated by
  chunking, not removed. → cap body size hard + treat truncation as retryable (already
  409 server-side).
- **`testConnection` is a plain-HTTP HEAD to the endpoint** — can only ever succeed on
  LAN/Tailscale; meaningless for iroh reach. → remove it (see P3), or make reach status
  derive from actual upload success only.

---

## P3 — Refocus the product: cut scope, collapse IA

The app should be a **pure collector**. Today it's collector + dashboard + diagnostics.

- **Cut the on-device dashboard (`TodayView`)** — HR chart, location map, audio
  timeline, contacts list. Visualization is the **box/web UI's** job. A collector
  shows: what's on, is it syncing, and errors. (Keep a tiny "today at a glance" only
  if there's a real offline use case — default: remove.)
- **Kill the second "manual" setup.** Two buttons in Settings → Server:
  - `ManualPairView` = real pairing (`consume`) ✅ keep.
  - `EndpointEditView` = edits a URL + `testConnection` ❌ delete (v0/bearer-era; it's
    what caused the "removed device, never re-added" mess and the "please check the URL").
- **Collapse Settings sprawl:** remove the read-only hardcoded "Auto Sync: Every 5
  minutes" (it's not configurable → noise); make the endpoint row non-editable info;
  always show "last attempt" so stalls are visible; remove nested `NavigationView`
  inside sheets (3 of them) to fix teardown/state hazards.
- **Reconsider `ActivityLogView` + 6 per-stream `*InfoView` sheets** — tinkerer/diagnostic
  UX. Keep a single compact "activity/errors" view; fold the six near-identical info
  sheets into one data-driven screen.
- **Onboarding IA:** default tab is "Today" with **no prompt to pair** → a new user
  sees nothing collecting and no guidance. Add a pair-first gate/banner; land on the
  collector, not a dashboard.

---

## P4 — One honest voice: copy/narrative

Pick one voice and make every claim true.

- **Purge v0 terminology:** "Server URL", "endpoint", "bearer", "token", "tunnel",
  "WireGuard", "linking code", "API key". Replace with one word for the target — **"box"** —
  everywhere.
- **Stop overpromising the app's role.** The app **collects**; the **box** does AI.
  Fix: "your AI is learning", "capture and **transcribe**", "**identify people** in your
  conversations", "unlocks new AI capabilities". State plainly: "Sends your data to your
  box, which analyzes it."
- **Remove false "standalone / optional sync"** claims in `WelcomeView` — there is no
  local-only mode; unpaired = uploads fail silently.
- **Fix misleading status:** "please check the URL" (wrong transport), "not reaching
  box" (say why + remedy), "Upload sent" (≠ delivered). Standardize state labels
  (Connected/Connecting…/… — one tense).
- **Hide jargon at the UI boundary:** never show `ios_mic`, `action_id`, "allowlist",
  "node id", "webhook".
- **Contradiction to fix:** audio described as both "30-second chunks" and "5-minute
  chunks" — pick the truth (30s w/ 2s overlap).

---

## P5 — Truth in docs

- Rewrite `apps/ios/CLAUDE.md`: drop the false guarantees ("zero data loss / zero
  races / 100% background reliability / 5-min sync"), the v0 "enter endpoint + API key"
  onboarding section, and align stream/cadence facts with the code.

---

## Suggested sequencing

1. **P0** (durability) — the product's reason to exist; do first, with the acceptance tests.
2. **P1** (crashes) — cheap, high-value stability.
3. **P3** (scope/IA) — deleting the dashboard + second-manual + settings cruft removes
   whole classes of bugs and shrinks the surface P4 has to fix.
4. **P4** (copy) — much smaller once P3 has cut screens.
5. **P2 leftovers** (dead candidate, testConnection removal) — fold into P3.
6. **P5** (docs) — last, reflecting the new reality.

Do P0+P1 as a "make it trustworthy" pass, then P3+P4 as a "make it focused" pass.
