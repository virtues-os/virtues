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
        /// This device's iroh secret seed (32-byte hex), generated at pairing.
        /// Its EndpointId is submitted to the box (to be allowlisted); the app
        /// builds its iroh endpoint from this seed to reach the box. This seed IS
        /// the device's credential — there is no bearer. Never leaves the device.
        case irohSeed = "virtues.iroh.seed"
    }

    // ─── iroh device seed (the device's only credential) ────────────────

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
