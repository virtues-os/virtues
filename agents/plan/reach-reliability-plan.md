# Reach reliability — "100% reachable whenever the box is up"

Goal: the user NEVER thinks about connectivity. If their box is up, the app reaches it — across
Wi-Fi↔cellular switches, LAN drops, and app suspend/resume — with **no force-quit, ever**.

## Root cause (verified, 2026-07-09)

The full-offline wedge (pages + chat + uploads all dead, only force-quit fixes it) is a **known open
upstream iroh bug: [n0-computer/iroh#4289](https://github.com/n0-computer/iroh/issues/4289)** (OPEN,
milestone v1.0.0). Chain of events:

1. iOS **suspends** the app (background) or the **network switches** (Wi-Fi↔cellular). iOS invalidates
   the app's UDP socket — it becomes *permanently broken* (`ENOTCONN`/`ENETDOWN`; Apple TN2277).
2. iroh's network monitor **does fire on iOS** (BSD route socket — iOS is first-class, not a stub) and
   **attempts a rebind**. But it tries **exactly once**, and at the instant of foreground the new
   interface often isn't ready → the rebind **fails**.
3. On that failed rebind, iroh **silently kills the EndpointDriver** — no error is returned to any
   caller, and there is **no `rebind()` API** to recover. The Endpoint is dead for the process's life.
4. **Our code never rebuilds the Endpoint.** `build_client()` runs once at launch;
   `drop_conn()` only clears the cached *Connection*, not the Endpoint's dead socket. So every re-dial
   rides the same dead socket → everything offline until force-quit (which builds a fresh Endpoint).

This is why my earlier `drop_conn`-on-drain-error fix was insufficient: it targets the Connection, but
the wedge is at the **Endpoint/socket** layer. The relay path can't save us either — relay reconnect
rides the *same* dead UDP socket.

## The proper fix — two layers (iroh maintainers' recommended mobile pattern)

Because `network_change()` **cannot report or fix a failed rebind** (#4289), a single API call isn't
"100%". We need poke-then-verify-then-rebuild.

### Layer 1 — poke iroh on every iOS network/lifecycle event
Call **`Endpoint::network_change()`** (async, idempotent, exists in iroh 1.0.1) whenever iOS tells us
something changed. This forces net-report → rebind → relay reconnect and heals the *common* case.
- **Triggers (Swift, in the reach plugin's iOS side):**
  - `NWPathMonitor.pathUpdateHandler` — any path change (Wi-Fi↔cellular, LAN up/down).
  - `UIApplication.didBecomeActiveNotification` — foreground (covers suspend/resume).
- Wire: new FFI `virtues_network_changed()` → `warm_client().endpoint.network_change().await`.

### Layer 2 — liveness watchdog that REBUILDS the Endpoint when Layer 1 fails
Because the wedge is silent, after poking we must actively verify and, if still dead, rebuild.
- **Probe:** bounded `tokio::time::timeout(5s, endpoint.online())` **and/or** a real round-trip
  (`probe_session`, already have it, 6s timeout).
- **If dead after a short retry → tear down and rebuild the whole client+Endpoint** via
  `build_client(&rec)`. This is the ONLY escape from a #4289-wedged socket today.
- **Identity is preserved:** the 32-byte SecretKey is persisted in the `FileStore` (`box.json`), so a
  rebuilt Endpoint has the **same EndpointId** — pairing/allowlist survive. (Verified: the seed is
  stored, not regenerated.)
- Wire: new FFI `virtues_rebuild_client()` → build fresh client from stored `rec`, swap `WARM_CLIENT`
  + the plugin's Tauri-state client, and make the loopback pick it up.

### Making the loopback rebuild-aware (the one structural change)
Today `serve_on` captures a **clone** of the client, so a rebuild wouldn't reach in-flight/ new proxy
connections. Fix: the loopback proxy fetches the **current** client from `WARM_CLIENT` per inbound
connection (or the serve loop is restarted on rebuild). Then swapping `WARM_CLIENT` instantly routes
new pages/chat/uploads through the fresh Endpoint. `WARM_CLIENT` becomes the single source of truth.

## Flow (steady state)
```
iOS event (NWPathMonitor change | didBecomeActive)
        │  virtues_network_changed()
        ▼
endpoint.network_change()          ← Layer 1: heals the common case
        │
        ▼  (bounded) probe: online()/probe_session
   reachable? ── yes ─► done (fresh socket, connected)
        │ no (retry once ~1s)
        ▼  still no
virtues_rebuild_client()           ← Layer 2: escape the #4289 wedge
   build_client(&rec)  [same SecretKey → same EndpointId]
   swap WARM_CLIENT + plugin client; loopback picks up new client
        ▼
   reconnected — pages/chat/uploads flow again, no force-quit
```

## Components / files
- **`crates/virtues-iroh/src/client.rs`** — expose `endpoint().network_change()` (add a
  `pub async fn network_change(&self)` passthrough) + already-added `drop_conn`/`path_kind`.
- **`apps/web/plugins/reach/src/ffi.rs`** — new `virtues_network_changed()` and
  `virtues_rebuild_client()` C-ABI (mirror the existing `virtues_enqueue`/`virtues_drain_blocking`).
- **`apps/web/plugins/reach/src/lib.rs`** — `WARM_CLIENT` as source of truth; a `rebuild_client()`
  that swaps it + plugin state; loopback reads current client per-connection.
- **`apps/web/plugins/reach/ios/Sources/…`** (new) — `NWPathMonitor` + `didBecomeActive` observer →
  `virtues_network_changed()` → bounded probe → `virtues_rebuild_client()` on failure. (The reach
  plugin currently has no iOS Swift; add a minimal `ReachNet.swift`.)
- **Status:** surface the recovery in This-device (path transitions / "reconnected") so it's visible.

## Notes / risks
- **Background budget:** a full rebuild is ~8–20s (endpoint bind + dial). Foreground/active triggers
  have time; a background wake rebuild is best-effort within the ~30s assertion — fine, it retries on
  next foreground.
- **Don't over-rebuild:** Layer 1 first; only rebuild when the probe proves it's actually dead —
  rebuilding on every flip is wasteful and drops in-flight streams.
- **Track upstream:** when iroh #4289 lands (rebind-retry + error propagation + background repair),
  Layer 2 can be dropped and we rely on `network_change()` alone. Pin the issue.
- Also verify the desktop path (`:7117` helper) benefits — same `WARM_CLIENT`/rebuild applies.

## Tailscale comparison — and why NOT a Network Extension (decided 2026-07-09)
Tailscale is the reference impl; its reliability comes from **two** things:
1. **A Network Extension** (`NEPacketTunnelProvider`) — its UDP stack runs in a VPN-tunnel process iOS
   keeps **alive while backgrounded**, so the socket isn't suspend-killed. Cost: a ~50 MB jetsam
   memory cap (forced a custom Go linker + stripped features).
2. **Rebind logic that's needed even WITH the NE** — [raggi PR #14551](https://github.com/tailscale/tailscale/pull/14551)
   rebinds on send `EPIPE`/`ENOTCONN` ("broken after sleep"), throttled 5 s; major-link-change rebind;
   DERP relay on a **separate TCP/443** that survives UDP death. iroh's netmon is the coarser cousin
   (250 ms vs 1 s debounce, no graded classification, **rebinds only on netmon events, not send
   errors** — exactly the #4289 blind spot).

**Decision: do NOT build a Network Extension.** The NE only buys "socket alive while the app is fully
suspended" — a guarantee our **wake-then-dial** model doesn't need (background sync happens when
location/BGTask *wake* the app → dial fresh; foreground just must never wedge). The portable half —
rebind-on-change/error — is what fixes our bug, and Tailscale proves it's necessary even inside an NE.
Our R1+R2 IS that half; our probe-fails→rebuild (R2) is the app-layer analog of their send-error→rebind.
Revisit an NE only if the product ever needs the box reachable while the app is fully closed (else the
[[project_apns_push_primitive]] wake-then-dial pattern covers it).

Tailscale-informed refinements folded into R1/R2:
- Trigger on **foreground + wake + every NWPathMonitor event**, not just "connectivity restored".
- **Throttle** rebuilds: poke+probe first; only rebuild when proven dead; don't rebuild on every flip.
- `IP_BOUND_IF` + NWPathMonitor→interface-hint live *inside* iroh — wait for upstream (#4289) or rely
  on our coarser whole-endpoint rebuild (which sidesteps the stale-interface problem).

## Phasing
- **R1 — Layer 1:** `network_change()` FFI + Swift NWPathMonitor/foreground triggers. Cheap; heals the
  common case immediately.
- **R2 — Layer 2:** rebuild FFI + loopback-reads-WARM_CLIENT + Swift watchdog (probe→rebuild). The
  "100%" guarantee.
- **R3 — polish:** visible reconnect state in This-device; desktop parity; metrics/logging.
