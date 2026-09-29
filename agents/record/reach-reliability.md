# Reach reliability

Written 2026-09-29. Built July to August 2026, shipped in the iOS app.
The goal was that an owner never thinks about connectivity: if the box is up,
the app reaches it across Wi-Fi and cellular switches, LAN drops and
suspend/resume, with no force-quit ever.

The code: `crates/virtues-iroh/src/client.rs` (`network_change`),
`apps/web/plugins/reach/src/lib.rs` (`recover_connection`, the warm client),
`apps/web/plugins/reach/src/ffi.rs` (`virtues_recover_connection`),
`crates/virtues-reach-client/src/proxy.rs` (the loopback), and
`apps/web/plugins/location-probe/ios/Sources/ReachMonitor.swift` (the
triggers). This is why they look the way they do.

## The wedge

The symptom was total: pages, chat and uploads all dead, and only a force-quit
brought them back. The cause is an open upstream bug,
[n0-computer/iroh#4289](https://github.com/n0-computer/iroh/issues/4289)
("Failed socket rebind kills noq EndpointDriver"), still open on 2026-09-29
with the repo on iroh 1.2.0.

1. iOS suspends the app, or the network switches, and invalidates the app's
   UDP socket. It stays broken (`ENOTCONN`/`ENETDOWN`).
2. iroh's network monitor does fire on iOS and does attempt a rebind. It tries
   exactly once, and at the instant of foreground the new interface is often
   not ready yet, so the rebind fails.
3. On that failure iroh silently kills the endpoint's driver. No error reaches
   any caller and there is no `rebind()` API. The endpoint is dead for the
   life of the process.
4. Our code built the endpoint once at launch. Dropping the cached connection
   did not help, because every re-dial rode the same dead socket. The relay
   could not save it either: relay reconnect uses the same socket.

A first fix that dropped the connection on drain errors was aimed at the wrong
layer. **The wedge is in the endpoint's socket, not the connection**, and the
only escape is a new endpoint.

## Two layers

`network_change()` cannot report or repair a failed rebind, so one API call is
not enough. The shape is poke, verify, rebuild.

- **Poke.** `Endpoint::network_change()` on every path change and every
  foreground: rebind, re-run net-report, reconnect the relay. This heals the
  common case.
- **Verify.** Wait 600 ms for it to settle, then run a real round trip
  (`probe_session`, bounded at 4 s). An authed or rejected answer both mean
  the socket works.
- **Rebuild.** If the probe fails, build a whole new client from the stored
  pairing record and swap it in as the warm client, then shut the old one
  down to free the dead socket.

**The rebuild keeps the same EndpointId.** The device's 32-byte seed is
persisted with the pairing (Keychain on iOS, a `0600` file elsewhere) and the
EndpointId is derived from it, so a rebuilt endpoint is the same device to the
box. Pairing and the box's allowlist survive, and a rebuild costs a dial, not
a re-pair. Had the key been regenerated, recovery would have looked like a
stranger and been refused.

## Structural choices

- **The warm client is the single source of truth.** The loopback, the upload
  drain and the FFI background drain all read it per use. The loopback in
  particular looks up the current client for each inbound connection rather
  than capturing one at startup, so a swap reroutes new pages, chat and
  uploads with no listener restart.
- **The loopback holds a connection up to 3 s when no client exists**, which
  is the state mid-rebuild or on resume from a parked endpoint. Browser
  fetches do not retry a reset, so dropping those connections would turn
  every resume into a burst of failed requests.
- **One FFI call does both layers.** Swift never decides whether to rebuild;
  it makes one blocking call off the main thread and logs the return code
  (0 healed, 1 rebuilt, negative on error). A static flag stops overlapping
  recoveries, and Swift coalesces bursts to one every 3 s, because a route
  flap and a foreground often fire together.
- **The triggers live in the location-probe plugin**, not the reach plugin.
  The reach plugin is Rust-only with no iOS lifecycle hook; location-probe is
  always on and already had one.
- **A parked endpoint is not revived in the background.** For battery, the
  endpoint is torn down between background drains. Recovery skips a
  backgrounded app with no warm client, since reviving it on every cellular
  hop in a pocket would bring back the keepalive traffic parking removed. On
  foreground it skips the poke and probe and builds immediately.

## Why not a Network Extension

Tailscale is the reference, and its reliability comes from two things: a
Network Extension, whose packet tunnel keeps a UDP stack alive while the app
is suspended (at the cost of a roughly 50 MB memory cap), and rebind logic
that it needs even with the extension (rebinding on send errors, throttled,
with its relay on a separate TCP connection that survives UDP death).

Decided 2026-07-09: no Network Extension. It buys only "socket alive while the
app is fully suspended," which a wake-then-dial model does not need:
background sync happens when location or a background task wakes the app, and
it dials fresh. What fixes the wedge is the portable half, rebind on change,
and Tailscale shows that half is necessary regardless. Probe-fails-then-rebuild
is the app-layer analog of its send-error rebind. Revisit only if the product
needs the box reachable while the app is fully closed.

## When to delete layer two

The rebuild exists only to work around #4289. When upstream retries the
rebind and reports failure, the rebuild and its probe can go, and the poke
alone suffices. Check the issue before any iroh upgrade.

Remaining polish (desktop has no trigger, no recovery counter, nothing in the
UI) is tracked in `agents/plan/reach-polish-plan.md`.
