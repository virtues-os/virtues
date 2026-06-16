# virtues-tunnel

In-app **userspace WireGuard** for paired Virtues clients. Brings up a
split-scope tunnel to the box's ULA entirely inside the app process and exposes
`dial(ip, port) -> byte stream`, so a client can speak plain HTTP to the box
without taking over the OS's single system-VPN slot.

Used by the **iOS app** via the generated XCFramework. The desktop keeps its
existing kernel/`utun` (gotatun) path; this crate is iOS-only for now. The only
shared surface across the two is `virtues-protocol` (bundle + SPKI) + x25519
key-gen.

## Why userspace (not NetworkExtension)

iOS allows only one *active* system VPN. A `NEPacketTunnelProvider` — even
split-tunnel — would seize that slot and disable the user's iCloud Private Relay
/ Nord / Express. Running the tunnel in-process takes **no** VPN slot, needs no
entitlement, prompts for nothing, and coexists with any other VPN. Only the
app's own calls to the box go through it.

## Architecture (the onetun pattern)

```
app HTTP ─> Tunnel::dial ─> smoltcp TCP socket
                              │ plaintext IP packets
                     defguard_boringtun Tunn (Noise)
                              │ encrypted datagrams
                          UDP socket ─> box [global IPv6]:51820
```

Inside the tunnel the box serves plain HTTP on its ULA (`internal_ip` /
`http_port` from the bundle); WireGuard provides confidentiality, so there's no
TLS in-tunnel (same model as the desktop reverse proxy).

| File | Role |
|------|------|
| `keys.rs` | x25519 pairing key-gen (send pubkey at pair, keep privkey) |
| `wg.rs` | `Tunn` wrapper: IP packets ⇄ encrypted WG datagrams, handshake/keepalive |
| `netstack.rs` | smoltcp virtual L3 device bridging IP packets to/from `wg.rs` |
| `tunnel.rs` | event loop + public `Tunnel` / `TunnelStream` (`Read`+`Write`) |
| `rendezvous.rs` | recover the box endpoint after an IPv6 prefix rotation |
| `ffi.rs` | uniffi surface the iOS XCFramework binds |

## Rendezvous (prefix-rotation recovery)

The box encrypts its current endpoint under a per-box key `K` (in the bundle's
`RendezvousParams`) and PUTs it to virtues-api; we GET + AES-256-GCM-decrypt it
with the same `K`. The blob shape and crypto exactly mirror the box side
(`virtues_core::virtues_api::rendezvous`) — see the round-trip test, which seals
with the box's construction and decrypts here.

## Build the iOS XCFramework

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
crates/virtues-tunnel/build-xcframework.sh
# → crates/virtues-tunnel/generated/VirtuesTunnel.xcframework
#   crates/virtues-tunnel/generated/virtues_tunnel.swift
```

The iOS app's CI lane runs this before building the app (it needs macOS +
Xcode; the headless Linux release CI can't).

## Verification status

- ✅ **Compiles** clean (host); workspace member.
- ✅ **Unit-tested**: key-gen round-trip; rendezvous decrypt against the box's
  exact seal format + fail-closed cases. `cargo test -p virtues-tunnel`.
- ⏳ **Needs a live box / device** (cannot be done in headless CI): WG handshake
  against a real peer, `dial` + HTTP round-trip, throughput, prefix-rotation
  recovery, and the generated Swift bindings linking in Xcode. These are
  exercised in Workstream C on a physical device against a real box.
