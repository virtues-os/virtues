//
//  KeychainStore.swift
//  Virtues
//
//  Secure storage for the server-issued bearer token (and any future
//  per-device secrets like the WireGuard private key). Pair-only auth
//  requires the bearer to live in the OS Keychain — not UserDefaults —
//  so a casual device backup doesn't carry the box's credential off the
//  hardware.
//
//  Accessibility is `whenUnlockedThisDeviceOnly`: the key is bound to
//  this physical device's hardware, never travels via iCloud Keychain or
//  device migration. If the user gets a new phone, they re-pair from the
//  box — that's the same defense the auth model promises everywhere.
//

import Foundation
import Security

/// Lightweight wrapper around the iOS Keychain. Stores secrets for the
/// Virtues pair-only auth model.
///
/// **Why a separate class:** the existing `DeviceConfiguration` is
/// `Codable` and persisted to `UserDefaults`. Putting the bearer there
/// would leak it into iCloud backups. The Keychain is the right primitive
/// for a credential we never want to leave the device.
final class KeychainStore {

    /// Singleton accessor — there's only one box paired at a time, no
    /// reason to manage multiple instances.
    static let shared = KeychainStore()

    private init() {}

    // ─── Keys ──────────────────────────────────────────────────────────

    private enum Key: String {
        /// Server-issued 32-byte hex bearer, returned once by
        /// `POST /api/pair/consume`. Sent as `Authorization: Bearer <token>`
        /// on every box API call.
        case bearerToken = "virtues.bearer"

        /// On-device-generated WireGuard private key (base64). Only set
        /// for tunnel-capable devices. Never sent to the box.
        case wgPrivateKey = "virtues.wg.privkey"

        /// The encrypted-at-rest WG bundle JSON we got back at pair time
        /// (server pubkey, allowed IPs, endpoint). Used to bring the tunnel
        /// up when needed.
        case wgBundle = "virtues.wg.bundle"

        /// Trust-on-first-use pin: the box's WG server public key (base64) seen
        /// at the first successful pair. A later pair offering a *different* key
        /// is refused unless the user explicitly confirms a rotation — catches a
        /// silent server-identity substitution on re-pair.
        case wgServerPin = "virtues.wg.serverpin"

        /// This device's iroh secret seed (32-byte hex), generated at pairing.
        /// Its EndpointId is submitted to the box (to be allowlisted); the app
        /// builds its iroh endpoint from this seed to reach the box. Never leaves
        /// the device.
        case irohSeed = "virtues.iroh.seed"
    }

    // ─── Bearer ────────────────────────────────────────────────────────

    func saveBearer(_ token: String) throws {
        try save(token.data(using: .utf8)!, for: .bearerToken)
    }

    func loadBearer() -> String? {
        guard let data = load(.bearerToken) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    func deleteBearer() {
        delete(.bearerToken)
    }

    // ─── WG keypair + bundle ───────────────────────────────────────────

    func saveWgPrivateKey(_ base64: String) throws {
        try save(base64.data(using: .utf8)!, for: .wgPrivateKey)
    }

    func loadWgPrivateKey() -> String? {
        guard let data = load(.wgPrivateKey) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    func saveWgBundle(_ json: Data) throws {
        try save(json, for: .wgBundle)
    }

    func loadWgBundle() -> Data? {
        load(.wgBundle)
    }

    func saveServerPin(_ publicKeyB64: String) throws {
        try save(publicKeyB64.data(using: .utf8)!, for: .wgServerPin)
    }

    func loadServerPin() -> String? {
        guard let data = load(.wgServerPin) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    // ─── iroh device seed ──────────────────────────────────────────────

    func saveIrohSeed(_ hex: String) throws {
        try save(hex.data(using: .utf8)!, for: .irohSeed)
    }

    func loadIrohSeed() -> String? {
        guard let data = load(.irohSeed) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// Wipe everything pair-related — used after a `/api/devices/:id`
    /// revoke reflects in the iOS app, or at the start of a new pair.
    func wipeAll() {
        delete(.bearerToken)
        delete(.wgPrivateKey)
        delete(.wgBundle)
        delete(.wgServerPin)
        delete(.irohSeed)
    }

    // ─── Primitives ────────────────────────────────────────────────────

    private func save(_ data: Data, for key: Key) throws {
        // Delete first so we don't have to worry about `errSecDuplicateItem`.
        delete(key)
        let attrs: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: key.rawValue,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
        ]
        let status = SecItemAdd(attrs as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.unhandled(status: status)
        }
    }

    private func load(_ key: Key) -> Data? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: key.rawValue,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &item)
        guard status == errSecSuccess else { return nil }
        return item as? Data
    }

    private func delete(_ key: Key) {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: key.rawValue,
        ]
        SecItemDelete(q as CFDictionary)
    }
}

enum KeychainError: Error, CustomStringConvertible {
    case unhandled(status: OSStatus)

    var description: String {
        switch self {
        case .unhandled(let status):
            return "Keychain error \(status)"
        }
    }
}
