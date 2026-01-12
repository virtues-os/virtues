//
//  KeychainHelper.swift
//  Virtues
//
//  Secure storage helper for sensitive data like tokens
//

import Foundation
import Security

/// Helper class for secure Keychain operations
final class KeychainHelper {
    static let shared = KeychainHelper()

    private let service = "com.virtues.ios"

    private init() {}

    // MARK: - Public API

    /// Save a string value to the Keychain
    /// - Parameters:
    ///   - value: The string value to store
    ///   - key: The key to store it under
    /// - Returns: Whether the operation succeeded
    @discardableResult
    func save(_ value: String, forKey key: String) -> Bool {
        guard let data = value.data(using: .utf8) else {
            print("❌ KeychainHelper: Failed to encode value for key: \(key)")
            return false
        }

        // Delete any existing item first
        delete(key)

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        ]

        let status = SecItemAdd(query as CFDictionary, nil)

        if status == errSecSuccess {
            print("✅ KeychainHelper: Saved value for key: \(key)")
            return true
        } else {
            print("❌ KeychainHelper: Failed to save value for key: \(key), status: \(status)")
            return false
        }
    }

    /// Retrieve a string value from the Keychain
    /// - Parameter key: The key to retrieve
    /// - Returns: The stored string value, or nil if not found
    func get(_ key: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        if status == errSecSuccess, let data = result as? Data {
            return String(data: data, encoding: .utf8)
        }

        return nil
    }

    /// Delete a value from the Keychain
    /// - Parameter key: The key to delete
    /// - Returns: Whether the operation succeeded (or item didn't exist)
    @discardableResult
    func delete(_ key: String) -> Bool {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key
        ]

        let status = SecItemDelete(query as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound
    }

    /// Check if a key exists in the Keychain
    /// - Parameter key: The key to check
    /// - Returns: Whether the key exists
    func exists(_ key: String) -> Bool {
        return get(key) != nil
    }

    // MARK: - Convenience Keys

    /// Key for device token storage
    static let deviceTokenKey = "device_token"
}
