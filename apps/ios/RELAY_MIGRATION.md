# iOS transport: iroh (QUIC P2P)

The iOS app reaches the box over **iroh** — the box is an iroh `Endpoint`
addressed by its Ed25519 EndpointId, reached LAN-direct, hole-punched, or via our
relay. There is no in-app WireGuard tunnel and no public box URL. This replaced
both the WireGuard model and the interim relay-HTTPS model.

## How it's wired

- **`Managers/Tunnel/BoxTransport.swift`** — an `actor` holding one warm
  `IrohTransport` (the Rust client, from `VirtuesIroh.xcframework`). `send()`
  serializes the caller's `URLRequest` to HTTP/1 bytes (`HTTPWire.swift`), sends
  them over a fresh iroh bi-stream, and parses the reply into
  `(Data, HTTPURLResponse)`. NetworkManager / BatchUploadCoordinator are
  unchanged above this line. Dialed lazily from the reach ticket; dropped +
  redialed on any transport error.
- **`Managers/Tunnel/HTTPWire.swift`** — HTTP/1.1 request serializer + response
  parser (origin-form target, `Host`, `Content-Length`, `Connection: close`).
- **`Managers/Tunnel/VirtuesIroh.swift`** — uniffi-generated Swift bindings for
  the `virtues-iroh-ffi` crate (`IrohTransport.dial/request/close`,
  `endpointIdFromSeed`). **Generated — do not edit by hand**; regenerate with the
  build script below.
- **Reach ticket**: `box_node_id` + `relay_url` live in `DeviceConfiguration`
  (UserDefaults, non-secret); this device's 32-byte iroh **seed** lives in the
  Keychain (`KeychainStore.irohSeed`). `DeviceManager.currentReachTicket()`
  resolves all three off any thread.
- **Pairing (consume, primary)**: `NetworkManager.consumePairToken` generates the
  device seed, derives its EndpointId (`endpointIdFromSeed`), and submits it as
  `device_node_id` so the box allowlists this device; it reads `box_node_id` +
  `relay_url` back and stores the ticket.
- **Pairing (provision, Mac→phone)**: the scanner accepts the box's v2 JSON QR
  `{ v:2, box_node_id, relay_url, bearer, credential_id, device_id }`
  (`QRScannerView.parseProvisionPayload` → `SettingsView.handleBundleScanResult`),
  stores bearer + ticket + a fresh seed, and registers the device's EndpointId
  via `POST /api/devices/self/node-id` (`registerSelfNodeId`, best-effort).

## Rebuilding the FFI (after changing `crates/virtues-iroh-ffi` or bumping iroh)

```sh
./crates/virtues-iroh-ffi/build-ios.sh
```

This builds the static lib for device + simulator, assembles
`crates/virtues-iroh-ffi/generated/VirtuesIroh.xcframework` (git-ignored build
artifact, referenced by the Xcode project), regenerates the Swift bindings, and
copies `VirtuesIroh.swift` into `apps/ios/Virtues/Managers/Tunnel/` (committed as
source). Requires the rustup iOS targets + Xcode. The app links
`SystemConfiguration.framework` (iroh network-interface discovery) via
`OTHER_LDFLAGS`.

## Known follow-up

- The **provision path** registers the device's EndpointId post-scan over iroh,
  which only lands once the box can reach the device — on a fresh provision the
  first attempt may fail and the user completes via the QR-consume path (primary).
  A backend tweak (accept the device node_id at provision time) would close this.
