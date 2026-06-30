# iOS relay migration — Xcode handoff

The Swift **networking + pairing** has been rewritten for the relay model (the
box is reached over plain HTTPS at the URL it returns at pair time; no in-app
WireGuard tunnel). The tunnel source files were **left in place** so the project
still compiles. Finish the cut in Xcode, where you have a compiler.

## What was already changed (compiles as-is)

- **`Managers/Tunnel/BoxTransport.swift`** — now a thin direct-HTTPS sender
  (`URLSession.data(for:)`); no longer references `VirtuesTunnelManager`.
- **`Managers/Data/NetworkManager.swift`**
  - `consumePairToken` no longer generates a WG keypair, sends `wg_public_key`,
    or verifies/stores a bundle. `expectedFingerprint` is accepted but unused
    (kept for call-site compatibility).
  - `PairConsumeRequest` dropped `wg_public_key`.
  - `PairConsumeResponse` gained `boxUrl` (`box_url` via `.convertFromSnakeCase`).
  - `confirmPairOnline` now uses `DeviceManager.shared.configuration.baseURL`.
- **`Views/SettingsView.swift`** — `handleQRScanResult` sets `apiEndpoint` to the
  box's relay URL (`response.boxUrl ?? endpoint`).
- **Box side**: `POST /api/pair/consume` now returns `box_url`
  (`https://<boxhash>.boxes.virtues.com`) when the box has a relay SNI.

## What still references the tunnel (delete/clean in Xcode)

These files are now vestigial. Remove them from the project (Xcode → delete +
"Move to Trash"), which also fixes the `.pbxproj` references:

- `Managers/Tunnel/VirtuesTunnelManager.swift`
- `Managers/Tunnel/virtues_tunnel.swift` (the boringtun/smoltcp FFI shim)
- `Views/ConnectionSettingsView.swift` (entirely a tunnel-status UI)

Plus remove the WireGuard XCFramework / `virtues_tunnel` binary target from the
project and any "Link Binary With Libraries" / "Embed Frameworks" entry for it.

## Remaining references to remove (the compiler will point at each)

After deleting the files above, these staying files still call into them — clean
them up (all are obsolete in the relay model):

- **`Views/SettingsView.swift`**
  - `handleBundleScanResult(_:)` — the `virtues-bundle:` off-LAN pairing path is
    obsolete (the relay URL works off-LAN directly). Delete the method and the QR
    branch that routes to it; keep only `handleQRScanResult` (the `/pair#t=`
    path).
  - `resetApp()` — delete the `VirtuesTunnelManager.shared.teardown()` line.
  - Remove any navigation to `ConnectionSettingsView`.
- **`Managers/Sync/BatchUploadCoordinator.swift`** — delete the
  `VirtuesTunnelManager.shared.teardown()` call (~line 221).
- **`Core/Keychain/KeychainStore.swift`** — `saveWgPrivateKey` / WG-private
  entries are now unused; remove once nothing references them (the `wipeAll`
  path can drop the WG key cleanup).
- **`Views/QRScannerView.swift`** — if it distinguishes `virtues-bundle:` QRs
  from `/pair#t=` URLs, drop the bundle branch.

## Provision-QR contract (Mac→phone hand-off) — NEW, implement the scanner

The desktop-relayed provision flow (an already-paired Mac asks the box to
provision the phone, then shows a QR) **replaces** the old `virtues-bundle:` WG
blob. The box now renders the QR of a compact JSON payload (see
`virtues-core/src/api/pair.rs` `provision_handler`):

```json
{ "v": 1,
  "box_url": "https://<boxhash>.virtues.ch",
  "bearer": "<device bearer>",
  "credential_id": "<credential id>",
  "device_id": "<device id>" }
```

The phone scanner must: parse this JSON, store `apiEndpoint = box_url` and the
`bearer` (same Keychain slots the `/pair#t=` path uses), and start uploading —
no WG, no key generation, no second round-trip to the box. The QR carries the
bearer in cleartext, so it has a short TTL (120s) and the provisioned device is
revoked if the user cancels before the phone comes online (the web modal handles
that side). The box only emits this QR once it is relay-registered (`box_url`
present); otherwise `qr_svg` is empty (there is no off-LAN address to hand off).

So `handleBundleScanResult` isn't just deleted — its QR branch is **repointed**
to a small JSON decoder for the payload above. The `ProvisionResponse` already
carries `box_url` too, if the web prefers to build the QR client-side via the
bundled `qrcode` lib instead of displaying `qr_svg` (either works; pick one).

## Sanity check after cleanup

1. `grep -rn 'VirtuesTunnelManager\|virtues_tunnel\|wg_public_key\|verifyAndStoreBundle\|BoxBundle' apps/ios/Virtues` → should return nothing.
2. Build for a device/simulator.
3. Pair against a relay-configured box: scanning the box's `/pair#t=` QR should
   store `apiEndpoint = https://<boxhash>.boxes.virtues.com` and uploads should
   succeed over HTTPS with `Authorization: Bearer <token>`.
