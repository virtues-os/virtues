import Foundation
import Security

struct Config: Codable {
    /// Server-issued device bearer (stored in the Keychain). Sent as
    /// `Authorization: Bearer <bearer>` on every webhook upload.
    let bearer: String
    let deviceId: String
    let apiEndpoint: String
    /// `function_name → action_id` map from pair-consume — the webhook targets.
    /// mac-source posts to `actionIds["mac_ingest"]`.
    let actionIds: [String: String]
    let createdAt: Date

    static let configDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".virtues")
    static let configFile = configDir.appendingPathComponent("config.json")

    // Keychain constants (account kept stable; it now stores the bearer)
    private static let keychainService = "com.virtues.collector"
    private static let keychainAccount = "device-token"

    // Private struct for JSON storage (secret bearer stays in the Keychain)
    private struct ConfigFile: Codable {
        let deviceId: String
        let apiEndpoint: String
        let actionIds: [String: String]
        let createdAt: Date
    }

    static func load() -> Config? {
        guard FileManager.default.fileExists(atPath: configFile.path) else {
            return nil
        }

        do {
            // Load config file (without the secret bearer)
            let data = try Data(contentsOf: configFile)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let cf = try decoder.decode(ConfigFile.self, from: data)

            // Load bearer from Keychain
            guard let bearer = loadTokenFromKeychain() else {
                print("⚠️ Config file exists but bearer not found in Keychain — re-run `init`")
                return nil
            }

            return Config(
                bearer: bearer,
                deviceId: cf.deviceId,
                apiEndpoint: cf.apiEndpoint,
                actionIds: cf.actionIds,
                createdAt: cf.createdAt
            )
        } catch {
            print("Error loading config: \(error)")
            return nil
        }
    }

    func save() throws {
        // Create directory if needed
        try FileManager.default.createDirectory(
            at: Config.configDir,
            withIntermediateDirectories: true
        )

        // Save bearer to Keychain
        try Self.saveTokenToKeychain(bearer)

        // Save config file (without the secret)
        let cf = ConfigFile(
            deviceId: deviceId,
            apiEndpoint: apiEndpoint,
            actionIds: actionIds,
            createdAt: createdAt
        )

        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = .prettyPrinted
        let data = try encoder.encode(cf)
        try data.write(to: Config.configFile)

        print("✅ Config saved (bearer stored securely in Keychain)")
    }
    
    static func delete() throws {
        // Delete token from Keychain
        deleteTokenFromKeychain()

        // Delete config file
        if FileManager.default.fileExists(atPath: configFile.path) {
            try FileManager.default.removeItem(at: configFile)
        }
    }

    // MARK: - Keychain Helpers

    private static func saveTokenToKeychain(_ token: String) throws {
        guard let tokenData = token.data(using: .utf8) else {
            throw ConfigError.networkError("Failed to encode token")
        }

        // First, try to update existing keychain item
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: keychainAccount
        ]

        let attributes: [String: Any] = [
            kSecValueData as String: tokenData
        ]

        let updateStatus = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)

        if updateStatus == errSecItemNotFound {
            // Item doesn't exist, create new one
            var newItem = query
            newItem[kSecValueData as String] = tokenData

            let addStatus = SecItemAdd(newItem as CFDictionary, nil)

            guard addStatus == errSecSuccess else {
                throw ConfigError.networkError("Failed to save token to Keychain: \(addStatus)")
            }
        } else if updateStatus != errSecSuccess {
            throw ConfigError.networkError("Failed to update token in Keychain: \(updateStatus)")
        }
    }

    private static func loadTokenFromKeychain() -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: keychainAccount,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        guard status == errSecSuccess,
              let data = result as? Data,
              let token = String(data: data, encoding: .utf8) else {
            return nil
        }

        return token
    }

    private static func deleteTokenFromKeychain() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: keychainAccount
        ]

        let status = SecItemDelete(query as CFDictionary)

        if status == errSecSuccess {
            print("✅ Token deleted from Keychain")
        } else if status == errSecItemNotFound {
            print("ℹ️ No token found in Keychain to delete")
        } else {
            print("⚠️ Failed to delete token from Keychain: \(status)")
        }
    }
    
    /// Pair this collector with the box via the unified `/api/pair/consume`
    /// flow (the same path the iOS app uses). Consumes a one-time pair token
    /// (from `virtues link` on the box) and returns the server-issued bearer +
    /// the `action_ids` map + the box endpoint.
    ///
    /// We declare `source = "mac"` so the box sets the credential's `source_id`
    /// to "mac" and `reconcile_templates` fans out the `mac_ingest` webhook
    /// action — the key we then POST uploads to. The box URL comes from
    /// `VIRTUES_API_URL` (the installer sets it), defaulting to the box's
    /// localhost port.
    static func pairConsume(token: String) async throws -> (bearer: String, deviceId: String, endpoint: String, actionIds: [String: String]) {
        let baseURL = ProcessInfo.processInfo.environment["VIRTUES_API_URL"] ?? "http://localhost:8000"
        let trimmed = baseURL.hasSuffix("/") ? String(baseURL.dropLast()) : baseURL
        let root = trimmed.hasSuffix("/api") ? String(trimmed.dropLast(4)) : trimmed

        guard let url = URL(string: "\(root)/api/pair/consume") else {
            throw ConfigError.invalidToken
        }

        let host = ProcessInfo.processInfo.hostName
        let body: [String: Any] = [
            "token": token,
            "kind": "desktop_app",
            "source": "mac",
            "label": host,
            "device_info": [
                "device_name": host,
                "os": "macos",
                "client": "virtues-collector",
            ],
        ]

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = 15
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw ConfigError.networkError("no HTTP response from \(root)")
        }
        guard http.statusCode == 200 else {
            let msg = String(data: data, encoding: .utf8) ?? "status \(http.statusCode)"
            throw ConfigError.networkError("pair/consume failed (\(http.statusCode)): \(msg)")
        }

        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let bearer = json["bearer"] as? String, !bearer.isEmpty,
              let deviceId = json["device_id"] as? String else {
            throw ConfigError.invalidToken
        }
        let actionIds = (json["action_ids"] as? [String: String]) ?? [:]
        return (bearer: bearer, deviceId: deviceId, endpoint: root, actionIds: actionIds)
    }
}

enum ConfigError: LocalizedError {
    case notConfigured
    case invalidToken
    case networkError(String)
    
    var errorDescription: String? {
        switch self {
        case .notConfigured:
            return "Not configured. Run 'virtues-collector init <token>' first."
        case .invalidToken:
            return "Invalid device token. Please check your token and try again."
        case .networkError(let message):
            return "Network error: \(message)"
        }
    }
}