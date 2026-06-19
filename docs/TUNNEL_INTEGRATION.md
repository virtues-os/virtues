# iOS tunnel integration (Workstream C/D)

The app now reaches the box over an **in-app userspace WireGuard tunnel** when
off the local network — no system VPN, no NetworkExtension, coexists with the
user's iCloud Private Relay / Nord / etc. The tunnel engine is the
`virtues-tunnel` Rust crate, bound via an XCFramework.

## What changed (all in the working tree)

| File | Change |
|------|--------|
| `Managers/Tunnel/VirtuesTunnelManager.swift` | **New.** Owns the FFI: pairing key-gen, bundle persistence, tunnel bring-up, status, SPKI. |
| `Managers/Tunnel/BoxTransport.swift` | **New.** Every box HTTP call goes here. A paired device ALWAYS tunnels (on-LAN and off) — HTTP/1.1 over `dial`, never plaintext; queues if the tunnel can't come up. |
| `Managers/Data/NetworkManager.swift` | Pairing generates a WG keypair + sends the pubkey + persists the returned `bundle`; `uploadData` routes via `BoxTransport`. |
| `Managers/Sync/BatchUploadCoordinator.swift` | action-ids refetch routed via `BoxTransport` (works off-LAN too). |
| `Views/ConnectionSettingsView.swift` | **New.** Tunnel status, box endpoint, SPKI fingerprint, test, forget. |
| `Views/SettingsView.swift` | "Connection" link added to the Server section. |
| `Models/DeviceConfiguration.swift` | Retired the legacy `deviceId`-as-bearer fallback. |

## Build + wire the XCFramework (one-time, on a Mac)

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
crates/virtues-tunnel/build-xcframework.sh
```

Then in `Virtues.xcodeproj`:

1. Drag `crates/virtues-tunnel/generated/VirtuesTunnel.xcframework` into the
   project; add it to the **Virtues** target (Frameworks, "Embed & Sign").
2. Add `crates/virtues-tunnel/generated/virtues_tunnel.swift` to the target
   (this is the uniffi-generated API; its symbols — `generateKeypair()`,
   `boxSpkiFingerprint(_:)`, `TunnelHandle`, `TunnelStreamHandle`,
   `PairKeypair` — are what the new Swift files reference, so no `import` is
   needed; they live in the app module).
3. Add the four new Swift files above to the target if Xcode didn't auto-add
   them.

> Until step 1–2 are done the new Swift files won't compile — the FFI symbols
> don't exist yet. That's expected; this is the device-build step.

## Verification (requires a device + a real box — can't be done in CI)

1. **On-LAN regression**: on the box's Wi-Fi, pair + confirm uploads still land
   (direct path, tunnel untouched).
2. **Off-LAN tunnel**: switch to cellular. Confirm a stream upload lands as an
   action run in the box logs — proves the WG handshake + `dial` + HTTP path.
3. **Settings → Connection**: status shows "connected"; the SPKI fingerprint
   matches what the box prints; "Test Tunnel" succeeds.
4. **Coexistence**: enable a third-party VPN (or iCloud Private Relay) and
   confirm it stays active while uploads still flow — proves we took no VPN slot.
5. **Prefix rotation (v1)**: if the box's IPv6 prefix changes, the baked endpoint
   goes stale and the device must re-pair (no auto re-resolution — see
   `docs/networking.md`).

## Background behavior — the location-keepalive model (C3 / R4)

The "iOS suspends a backgrounded app and reclaims its sockets" wall does **not**
apply here, because this app holds **continuous background location**
(`allowsBackgroundLocationUpdates = true`, `pausesLocationUpdatesAutomatically =
false`, Always authorization). That keeps the *process alive* in the background,
so its UDP socket and the in-process tunnel survive — no NetworkExtension needed.

Per cycle:

1. Background location keeps the app awake.
2. `BatchUploadCoordinator`'s 5-minute `ReliableTimer` fires `performUpload()`.
3. Box-bound requests go through `BoxTransport`, which always uses the WG tunnel
   (brought up lazily on first use) — on-LAN and off — never plaintext.
4. When the burst finishes, `performUpload()`'s `defer` calls
   `VirtuesTunnelManager.shared.teardown()` — the tunnel is dropped so we don't
   hold an idle WG keepalive between cycles (the dominant battery cost per the
   audit research). The next off-LAN burst re-establishes it (~1 RTT).

Gaps (no LAN *and* no location runtime, e.g. app force-quit) are covered by the
SQLite queue: data is never lost, it flushes on the next cycle, foreground, or
return to home Wi-Fi. Off-LAN background uploads are therefore best-effort but
real — strictly better than before (off-LAN never worked), with no VPN slot taken.
