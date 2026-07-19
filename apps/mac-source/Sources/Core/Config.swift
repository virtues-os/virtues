import Foundation
import Security

/// Collector configuration. Auth is the device's iroh key — the 32-byte seed in
/// the Keychain — not a bearer. Uploads go to the box over iroh (`BoxTransport`)
/// using the reach ticket (`boxNodeId` + `relayUrl`); the box authenticates by
/// the allowlisted EndpointId derived from the seed.
struct Config: Codable {
    let deviceId: String
    /// The LAN origin used at pair time (`/api/pair/consume`). Kept for reference
    /// / re-pair; uploads no longer use it (they go over iroh).
    let apiEndpoint: String
    /// `function_name → action_id` map from pair-consume — the webhook targets.
    /// mac-source posts to `actionIds["mac_ingest"]`.
    let actionIds: [String: String]
    /// The box's iroh reach ticket: EndpointId + relay URL. Dialed by BoxTransport.
    let boxNodeId: String
    let relayUrl: String
    let createdAt: Date
    /// This device's iroh secret seed (32-byte hex). Loaded from the Keychain;
    /// derives the EndpointId submitted to the box + dials the transport. Never
    /// leaves the machine except as its public EndpointId.
    let deviceSeed: String

    static let configDir = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".virtues")
    static let configFile = configDir.appendingPathComponent("config.json")

    // Keychain: the account now stores the iroh seed (was the device bearer).
    private static let keychainService = "com.virtues.collector"
    private static let keychainAccount = "device-iroh-seed"

    // On-disk JSON (non-secret; the seed stays in the Keychain).
    private struct ConfigFile: Codable {
        let deviceId: String
        let apiEndpoint: String
        let actionIds: [String: String]
        let boxNodeId: String
        let relayUrl: String
        let createdAt: Date
    }

    static func load() -> Config? {
        guard FileManager.default.fileExists(atPath: configFile.path) else {
            return nil
        }
        do {
            let data = try Data(contentsOf: configFile)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let cf = try decoder.decode(ConfigFile.self, from: data)

            guard let seed = loadSeedFromKeychain() else {
                print("⚠️ Config file exists but the iroh seed isn't in the Keychain — re-run `init`")
                return nil
            }

            return Config(
                deviceId: cf.deviceId,
                apiEndpoint: cf.apiEndpoint,
                actionIds: cf.actionIds,
                boxNodeId: cf.boxNodeId,
                relayUrl: cf.relayUrl,
                createdAt: cf.createdAt,
                deviceSeed: seed
            )
        } catch {
            print("Error loading config: \(error)")
            return nil
        }
    }

    func save() throws {
        try FileManager.default.createDirectory(
            at: Config.configDir,
            withIntermediateDirectories: true
        )
        try Self.saveSeedToKeychain(deviceSeed)

        let cf = ConfigFile(
            deviceId: deviceId,
            apiEndpoint: apiEndpoint,
            actionIds: actionIds,
            boxNodeId: boxNodeId,
            relayUrl: relayUrl,
            createdAt: createdAt
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = .prettyPrinted
        let data = try encoder.encode(cf)
        try data.write(to: Config.configFile)

        print("✅ Config saved (iroh seed stored securely in Keychain)")
    }

    static func delete() throws {
        deleteSeedFromKeychain()
        if FileManager.default.fileExists(atPath: configFile.path) {
            try FileManager.default.removeItem(at: configFile)
        }
    }

    /// Wire the loaded reach ticket + seed into the transport. Call once at start.
    func activateTransport() async {
        await BoxTransport.shared.configure(boxNodeId: boxNodeId, relayUrl: relayUrl, seed: deviceSeed)
    }

    // MARK: - Keychain Helpers

    private static func saveSeedToKeychain(_ seed: String) throws {
        guard let seedData = seed.data(using: .utf8) else {
            throw ConfigError.networkError("Failed to encode iroh seed")
        }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: keychainAccount
        ]
        let attributes: [String: Any] = [kSecValueData as String: seedData]
        let updateStatus = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if updateStatus == errSecItemNotFound {
            var newItem = query
            newItem[kSecValueData as String] = seedData
            let addStatus = SecItemAdd(newItem as CFDictionary, nil)
            guard addStatus == errSecSuccess else {
                throw ConfigError.networkError("Failed to save iroh seed to Keychain: \(addStatus)")
            }
        } else if updateStatus != errSecSuccess {
            throw ConfigError.networkError("Failed to update iroh seed in Keychain: \(updateStatus)")
        }
    }

    private static func loadSeedFromKeychain() -> String? {
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
              let seed = String(data: data, encoding: .utf8) else {
            return nil
        }
        return seed
    }

    private static func deleteSeedFromKeychain() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: keychainAccount
        ]
        let status = SecItemDelete(query as CFDictionary)
        if status == errSecSuccess {
            print("✅ iroh seed deleted from Keychain")
        } else if status == errSecItemNotFound {
            print("ℹ️ No iroh seed found in Keychain to delete")
        } else {
            print("⚠️ Failed to delete iroh seed from Keychain: \(status)")
        }
    }

    /// Generate a fresh 32-byte iroh seed (hex).
    static func generateSeed() -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    /// Result of a successful pair-consume.
    struct Paired {
        let deviceId: String
        let endpoint: String
        let actionIds: [String: String]
        let boxNodeId: String
        let relayUrl: String
        let seed: String
    }

    /// Pair this collector with the box via the unified `/api/pair/consume` flow
    /// (the same public, token-gated route the iOS app uses — no bearer). We
    /// generate an iroh keypair, submit its EndpointId (`device_node_id`) so the
    /// box allowlists it, declare `source = "mac"` so `reconcile_templates` fans
    /// out the `mac_ingest` webhook action (anchored on this device), and read
    /// back the box's reach ticket for uploads over iroh. Consume runs over the
    /// LAN origin (`VIRTUES_API_URL`); everything after goes over iroh.
    static func pairConsume(token: String) async throws -> Paired {
        let baseURL = ProcessInfo.processInfo.environment["VIRTUES_API_URL"] ?? "http://localhost:8000"
        let trimmed = baseURL.hasSuffix("/") ? String(baseURL.dropLast()) : baseURL
        let root = trimmed.hasSuffix("/api") ? String(trimmed.dropLast(4)) : trimmed

        guard let url = URL(string: "\(root)/api/pair/consume") else {
            throw ConfigError.invalidToken
        }

        let seed = generateSeed()
        let nodeId: String
        do {
            nodeId = try endpointIdFromSeed(deviceSeedHex: seed)
        } catch {
            throw ConfigError.networkError("failed to derive iroh EndpointId: \(error)")
        }

        let host = ProcessInfo.processInfo.hostName
        let body: [String: Any] = [
            "token": token,
            "kind": "desktop_app",
            "source": "mac",
            "label": host,
            "device_node_id": nodeId,
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
              let deviceId = json["device_id"] as? String,
              let boxNodeId = json["box_node_id"] as? String, !boxNodeId.isEmpty,
              let relayUrl = json["relay_url"] as? String, !relayUrl.isEmpty else {
            throw ConfigError.networkError(
                "pair/consume response missing device_id or box reach ticket "
                + "(is the box's iroh endpoint up?)"
            )
        }
        let actionIds = (json["action_ids"] as? [String: String]) ?? [:]
        return Paired(
            deviceId: deviceId,
            endpoint: root,
            actionIds: actionIds,
            boxNodeId: boxNodeId,
            relayUrl: relayUrl,
            seed: seed
        )
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
            return "Invalid pair token. Please check your token and try again."
        case .networkError(let message):
            return "Network error: \(message)"
        }
    }
}
