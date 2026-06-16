//
//  VirtuesTunnelManager.swift
//  Virtues
//
//  Owns the in-app userspace WireGuard tunnel (the `virtues-tunnel` Rust crate,
//  bound via the VirtuesTunnel XCFramework). This is the single place that
//  touches the FFI: pairing key generation, bundle persistence, bringing the
//  tunnel up on demand, status, and SPKI verification.
//
//  Why a tunnel at all: under the IPv6-direct WG/SPKI doctrine the box is only
//  reachable over WireGuard when off the local network. We run WG entirely
//  in-process (no NetworkExtension), so it takes no system-VPN slot and
//  coexists with the user's iCloud Private Relay / Nord / etc. — only the app's
//  own calls to the box go through it. See crates/virtues-tunnel/README.md.
//
//  NOTE: the symbols `generateKeypair()`, `boxSpkiFingerprint(...)`,
//  `TunnelHandle`, `TunnelStreamHandle`, `PairKeypair` come from the
//  uniffi-generated `virtues_tunnel.swift` (built by
//  crates/virtues-tunnel/build-xcframework.sh and added to this target). Until
//  that XCFramework is wired into the Xcode project this file won't compile —
//  that's expected; it's the Workstream-C device-build step.
//

import Foundation

/// Minimal decode of the box's PairingBundle — only the fields the app needs to
/// dial and to display verification info. The full bundle JSON is handed to the
/// Rust FFI verbatim; this is just for Swift-side use. Decoded with
/// `.convertFromSnakeCase`, so keys map from the box's snake_case wire shape.
struct BoxBundle: Decodable {
    struct Wg: Decodable {
        let serverPublicKey: String
        let serverEndpoint: String
        let clientAddress: String
        let serverAddress: String
    }
    struct Rendezvous: Decodable {
        let url: String
    }
    let wg: Wg
    let internalHost: String
    let internalIp: String
    let httpPort: UInt16
    let rendezvous: Rendezvous
}

final class VirtuesTunnelManager {
    static let shared = VirtuesTunnelManager()
    private init() {}

    /// Serializes bring-up + handle access. The handle itself is internally
    /// thread-safe (Rust side), but we guard creation so two upload tasks don't
    /// race to build two tunnels.
    private let lock = NSLock()
    private var handle: TunnelHandle?

    // MARK: - Pairing

    /// Generate a fresh Curve25519 keypair, persist the private key in the
    /// Keychain, and return the **public** key to send to the box as
    /// `wg_public_key`. Called once per pair.
    func generateAndStorePairKeypair() throws -> String {
        let kp = generateKeypair()
        try KeychainStore.shared.saveWgPrivateKey(kp.privateKeyB64)
        return kp.publicKeyB64
    }

    /// Verify and persist the raw PairingBundle JSON returned by
    /// `/api/pair/consume`.
    ///
    /// Two checks gate storage (both throw on failure, aborting pairing):
    ///   1. **Out-of-band fingerprint** — if the QR carried the box's SPKI
    ///      fingerprint, the bundle's WG server key must hash to it. The QR is a
    ///      channel a LAN MITM can't sit on, so this defeats a substituted-key
    ///      attack on the (plaintext) pairing HTTP response.
    ///   2. **TOFU** — the server key must match the one pinned at first pair; a
    ///      changed key is refused (an explicit "Forget" / re-pair is required to
    ///      rotate).
    func verifyAndStoreBundle(_ json: Data, expectedFingerprint: String?) throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let bundle = try decoder.decode(BoxBundle.self, from: json)
        let serverKey = bundle.wg.serverPublicKey

        if let expected = expectedFingerprint, !expected.isEmpty {
            let actual = try boxSpkiFingerprint(serverPublicKeyB64: serverKey)
            guard actual == expected else {
                throw TunnelSetupError.fingerprintMismatch
            }
        }

        if let pinned = KeychainStore.shared.loadServerPin(), pinned != serverKey {
            throw TunnelSetupError.serverKeyChanged
        }

        try KeychainStore.shared.saveWgBundle(json)
        try? KeychainStore.shared.saveServerPin(serverKey)
        // A new bundle invalidates any live tunnel built from the old one.
        teardown()
    }

    /// True once we have both halves needed to bring a tunnel up.
    var canBringUp: Bool {
        KeychainStore.shared.loadWgBundle() != nil
            && KeychainStore.shared.loadWgPrivateKey() != nil
    }

    // MARK: - Lifecycle

    /// Bring the tunnel up (idempotent) and return the live handle. Throws if no
    /// bundle/key is stored or the Rust side rejects the bundle.
    @discardableResult
    func bringUp() throws -> TunnelHandle {
        lock.lock()
        defer { lock.unlock() }
        if let h = handle { return h }

        guard let bundleData = KeychainStore.shared.loadWgBundle(),
              let bundleJson = String(data: bundleData, encoding: .utf8),
              let privKey = KeychainStore.shared.loadWgPrivateKey()
        else {
            throw TunnelSetupError.notPaired
        }
        let h = try TunnelHandle(bundleJson: bundleJson, privateKeyB64: privKey)
        handle = h
        return h
    }

    /// Drop the live tunnel (its Rust `Drop` shuts the background loop down).
    /// Called on a new pair, on revoke, or when entering deep background.
    func teardown() {
        lock.lock()
        defer { lock.unlock() }
        handle = nil
    }

    // MARK: - Introspection

    /// Coarse status string from the live handle, or "idle" if not up.
    func statusString() -> String {
        lock.lock()
        defer { lock.unlock() }
        return handle?.status() ?? "idle"
    }

    /// Decoded bundle for dial target + display. Nil if unpaired/corrupt.
    func boxBundle() -> BoxBundle? {
        guard let data = KeychainStore.shared.loadWgBundle() else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try? decoder.decode(BoxBundle.self, from: data)
    }

    /// SPKI fingerprint (`sha256-…`) of the box's WG public key, for the user to
    /// compare against what's printed on the box. Nil if unpaired.
    func spkiFingerprint() -> String? {
        guard let b = boxBundle() else { return nil }
        return try? boxSpkiFingerprint(serverPublicKeyB64: b.wg.serverPublicKey)
    }
}

enum TunnelSetupError: LocalizedError {
    case notPaired
    case fingerprintMismatch
    case serverKeyChanged
    var errorDescription: String? {
        switch self {
        case .notPaired:
            return "No tunnel credentials — pair the device first."
        case .fingerprintMismatch:
            return "Box identity check failed — the server key didn't match the "
                + "fingerprint in the pairing code. Someone may be intercepting "
                + "the connection. Pairing was cancelled."
        case .serverKeyChanged:
            return "This box's identity has changed since you last paired. If you "
                + "intended to re-pair a reinstalled box, tap Forget in "
                + "Settings → Connection first, then scan again."
        }
    }
}
