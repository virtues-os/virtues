//
//  NetworkManager.swift
//  Virtues
//
//  Handles all network communication with retry logic
//

import Foundation
import UIKit
import CryptoKit

enum NetworkError: LocalizedError {
    case invalidURL
    case invalidToken              // 401 - requires re-auth
    case serverError(Int)          // 5xx - transient, retry with backoff
    case timeout                   // transient, retry
    case noConnection              // transient, retry when online
    case decodingError
    case rateLimited(retryAfter: TimeInterval)  // 429 - back off, don't break circuit
    case badRequest(message: String)            // 400 - permanent fail, don't retry
    case forbidden                              // 403 - permanent fail, don't retry
    case notProcessed(status: String)           // 2xx/409 but not a durable success - retry, keep data
    case unknown(Error)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            return "Invalid API endpoint URL"
        case .invalidToken:
            return "Invalid device token (E002)"
        case .serverError(let code):
            return "Server error: \(code) (E003)"
        case .timeout:
            return "Network timeout (E001)"
        case .noConnection:
            return "No internet connection"
        case .decodingError:
            return "Failed to decode response"
        case .rateLimited(let retryAfter):
            return "Rate limited - retry after \(Int(retryAfter))s (E004)"
        case .badRequest(let message):
            return "Bad request: \(message) (E005)"
        case .forbidden:
            return "Access forbidden (E006)"
        case .notProcessed(let status):
            return "Upload not processed (status: \(status)) — will retry"
        case .unknown(let error):
            return "Unknown error: \(error.localizedDescription)"
        }
    }

}

class NetworkManager: ObservableObject {
    static let shared = NetworkManager()
    
    private let session: URLSession
    private let timeout: TimeInterval = 30.0
    private init() {
        let configuration = URLSessionConfiguration.default
        configuration.timeoutIntervalForRequest = timeout
        configuration.timeoutIntervalForResource = timeout
        configuration.waitsForConnectivity = true
        configuration.allowsCellularAccess = true
        
        self.session = URLSession(configuration: configuration)
    }
    
    // MARK: - Data Upload
    
    func uploadData<T: Encodable>(_ data: T, deviceToken: String, endpoint: URL) async throws -> UploadResponse {
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(deviceToken)", forHTTPHeaderField: "Authorization")
        
        // Encode data
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        request.httpBody = try encoder.encode(data)
        
        do {
            // Direct on-LAN, tunnel fallback off-LAN. BoxTransport returns the
            // HTTPURLResponse directly (no optional cast needed).
            let (data, httpResponse) = try await BoxTransport.shared.send(request, session: session)

            switch httpResponse.statusCode {
            case 200...299:
                let resp = try JSONDecoder().decode(UploadResponse.self, from: data)
                // A 2xx alone does NOT mean the batch was ingested. The box
                // returns 200 only for status "success"; any other status that
                // somehow arrives with a 2xx (e.g. "running", or a "skipped"
                // from an older server) means the records were NOT durably
                // written. Treat anything but "success" as retryable so the
                // caller keeps the data in its queue instead of deleting it.
                guard (resp.status ?? "") == "success" else {
                    throw NetworkError.notProcessed(status: resp.status ?? "unknown")
                }
                return resp
            case 409:
                // Box skipped the run (concurrency gate / falsy condition).
                // The payload was not ingested — retryable, keep the data.
                let status = (try? JSONDecoder().decode(UploadResponse.self, from: data))?.status ?? "skipped"
                throw NetworkError.notProcessed(status: status)
            case 401:
                throw NetworkError.invalidToken
            case 400:
                // Parse error message from response if available
                let message: String
                if let errorResponse = try? JSONDecoder().decode(ErrorResponse.self, from: data) {
                    message = errorResponse.error
                } else if let bodyString = String(data: data, encoding: .utf8) {
                    message = bodyString
                } else {
                    message = "Invalid request data"
                }
                throw NetworkError.badRequest(message: message)
            case 403:
                throw NetworkError.forbidden
            case 429:
                // Parse Retry-After header or use default
                let retryAfter: TimeInterval
                if let retryAfterHeader = httpResponse.value(forHTTPHeaderField: "Retry-After"),
                   let seconds = TimeInterval(retryAfterHeader) {
                    retryAfter = seconds
                } else {
                    retryAfter = 60  // Default to 60 seconds
                }
                throw NetworkError.rateLimited(retryAfter: retryAfter)
            case 500...599:
                throw NetworkError.serverError(httpResponse.statusCode)
            default:
                throw NetworkError.unknown(NSError(domain: "HTTP \(httpResponse.statusCode)", code: httpResponse.statusCode))
            }
        } catch {
            if let urlError = error as? URLError {
                switch urlError.code {
                case .timedOut:
                    throw NetworkError.timeout
                case .notConnectedToInternet, .networkConnectionLost:
                    throw NetworkError.noConnection
                default:
                    throw NetworkError.unknown(error)
                }
            }
            
            if error is NetworkError {
                throw error
            }
            
            throw NetworkError.unknown(error)
        }
    }
    
    // MARK: - Pairing (v1: unified pair-only)

    /// Complete pairing via the unified v1 pair-only flow.
    /// `POST {endpoint}/api/pair/consume` with `kind = "mobile_app"`. The
    /// server creates the `app_device` row, mints a server-issued bearer
    /// (returned ONCE in the response), and runs the action fan-out so the
    /// device knows the `function_name → action_id` map.
    ///
    /// On success the bearer is written to the Keychain via
    /// `KeychainStore.shared.saveBearer(...)`. The caller is responsible for
    /// persisting `actionIds` + `apiEndpoint` into `DeviceConfiguration`.
    ///
    /// - Parameters:
    ///   - endpoint: Box root URL (e.g. `https://virtues.local`). Trailing
    ///     `/` or `/api` is tolerated and stripped.
    ///   - pairToken: The 24-byte hex token from the QR / `/pair#t=...` URL.
    ///   - deviceId: A device-local label (UUID) for the Devices page — NOT
    ///     used as a credential.
    /// - Returns: `(credentialId, actionIds)` for the caller to persist.
    func consumePairToken(
        endpoint: String,
        pairToken: String,
        deviceId: String,
        expectedFingerprint: String? = nil
    ) async throws -> PairConsumeResponse {
        let baseURL = endpoint.hasSuffix("/") ? String(endpoint.dropLast()) : endpoint
        let root = baseURL.hasSuffix("/api") ? String(baseURL.dropLast(4)) : baseURL

        guard let url = URL(string: "\(root)/api/pair/consume") else {
            throw NetworkError.invalidURL
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = 15.0

        // Capture device info on MainActor (UIDevice properties are MainActor-isolated)
        let deviceName = await UIDevice.current.name
        let osVersion = await UIDevice.current.systemVersion
        let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String

        let deviceInfo = PairingDeviceInfo(
            device_id: deviceId,
            device_name: deviceName,
            device_model: Self.modelIdentifier,
            os_version: osVersion,
            app_version: appVersion,
            timezone: TimeZone.current.identifier
        )

        // iroh model: this device has its own iroh identity. Generate (or reuse) a
        // 32-byte seed, derive its EndpointId, and submit it so the box allowlists
        // this device for iroh reach. The seed stays in the Keychain; only the
        // public EndpointId leaves the device.
        let seed = Self.ensureIrohSeed()
        let deviceNodeId = seed.flatMap { try? endpointIdFromSeed(deviceSeedHex: $0) }

        // Stable per-attempt idempotency key: if the response is lost and we
        // retry below, the box replays the SAME bearer instead of failing on the
        // now-consumed token.
        let idempotencyKey = UUID().uuidString
        let body = PairConsumeRequest(
            token: pairToken,
            kind: "mobile_app",
            label: deviceName,
            device_info: deviceInfo,
            device_node_id: deviceNodeId,
            idempotency_key: idempotencyKey
        )
        let encoder = JSONEncoder()
        request.httpBody = try encoder.encode(body)

        // Retry ONCE on a transport error (lost response) with the same body →
        // same idempotency key → the box re-returns the original bearer.
        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            (data, response) = try await session.data(for: request)
        }

        guard let httpResponse = response as? HTTPURLResponse else {
            throw NetworkError.unknown(NSError(domain: "Invalid response", code: 0))
        }

        switch httpResponse.statusCode {
        case 200...299:
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let parsed = try decoder.decode(PairConsumeResponse.self, from: data)
            // Park the bearer in the Keychain immediately — it's returned
            // exactly once by the server and we never want it to live in
            // process memory longer than this function.
            if let bearer = parsed.bearer, !bearer.isEmpty {
                try? KeychainStore.shared.saveBearer(bearer)
            }
            // iroh model: the box's identity is its Ed25519 EndpointId, returned
            // in the reach ticket (`box_node_id`) and dialed with mutual-key auth
            // (no CA). There's no separate fingerprint to pin out-of-band, so
            // `expectedFingerprint` is accepted for call-site compatibility but
            // unused. The caller persists `boxNodeId`/`relayUrl` as the ticket.
            _ = expectedFingerprint
            return parsed
        case 401:
            throw NetworkError.badRequest(message: "Pair token is invalid, expired, or already used. Get a fresh one from `virtues link` on the box.")
        case 400:
            let message: String
            if let errorResponse = try? JSONDecoder().decode(ErrorResponse.self, from: data) {
                message = errorResponse.error
            } else {
                message = "Pair request was malformed"
            }
            throw NetworkError.badRequest(message: message)
        case 500...599:
            throw NetworkError.serverError(httpResponse.statusCode)
        default:
            throw NetworkError.unknown(NSError(domain: "HTTP \(httpResponse.statusCode)", code: httpResponse.statusCode))
        }
    }

    /// Best-effort: make one authenticated call to the box over the freshly-built
    /// tunnel right after a relayed-bundle import. Its only job is to bump the
    /// box's `last_seen_at` so the relaying device's "+ Add Device" UI flips from
    /// "waiting" to "paired" immediately (the box's `provision-status` poll keys
    /// off `last_seen_at > paired_at`). Never throws — if the tunnel isn't up yet
    /// the next scheduled upload will register liveness anyway.
    func confirmPairOnline() async {
        guard let base = DeviceManager.shared.configuration.baseURL,
              let bearer = KeychainStore.shared.loadBearer(),
              let url = URL(string: "\(base.absoluteString)/api/devices/action-ids")
        else { return }

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        request.timeoutInterval = 15
        _ = try? await BoxTransport.shared.send(request, session: session)
    }

    /// Best-effort: re-pull the box's CURRENT reach ticket and persist it, so a
    /// device whose `relay_url` changed (or that paired before the box had relay
    /// reach) self-heals without a re-pair. Uses the existing iroh transport, so
    /// it only helps while the current ticket still reaches the box (e.g. the
    /// relay URL rotated); a fully-broken ticket still needs a re-pair. Never
    /// throws; never clears a good ticket if the box reports no reach.
    func refreshReach() async {
        guard let base = DeviceManager.shared.configuration.baseURL,
              let bearer = KeychainStore.shared.loadBearer(),
              let url = URL(string: "\(base.absoluteString)/api/devices/self/reach")
        else { return }
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        request.timeoutInterval = 15
        guard let (data, http) = try? await BoxTransport.shared.send(request, session: session),
              http.statusCode == 200 else { return }
        struct ReachResponse: Decodable { let boxNodeId: String?; let relayUrl: String? }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let reach = try? decoder.decode(ReachResponse.self, from: data),
              let node = reach.boxNodeId, !node.isEmpty else { return }
        await MainActor.run {
            DeviceManager.shared.updateReach(boxNodeId: node, relayUrl: reach.relayUrl)
        }
    }

    // MARK: - Link a device (new-device side, fully-remote enrollment)

    /// atlas base URL — the new device has no box reach yet, so it talks to atlas
    /// (public) to resolve the box + wait for approval. Prod default.
    private static let atlasBaseURL = "https://atlas.virtues.com"

    /// Run the full link-a-device flow: resolve the box via atlas, wait for the
    /// voucher to approve, then pull the bearer from the box over iroh. On success
    /// the device is configured + reachable. Throws with a user-facing message.
    func linkDevice(code rawCode: String) async throws {
        let alnum = rawCode.uppercased().filter { $0.isLetter || $0.isNumber }
        guard alnum.count == 10 else {
            throw NetworkError.badRequest(message: "That code doesn't look right — it should be 10 characters.")
        }
        let code = "\(alnum.prefix(5))-\(alnum.suffix(5))"
        let codeHash = Self.sha256Hex(code)

        // This device's iroh identity + a MAC binding its EndpointId to the code
        // (so a tampering coordinator can't swap it without the code).
        guard let seed = Self.ensureIrohSeed(),
              let endpointId = try? endpointIdFromSeed(deviceSeedHex: seed) else {
            throw NetworkError.unknown(NSError(domain: "iroh seed", code: 0))
        }
        let mac = Self.hmacSha256Hex(key: code, message: endpointId)

        // 1. Submit our EndpointId + MAC, learn the box's reach.
        let reach = try await atlasLinkLookup(codeHash: codeHash, endpointId: endpointId, mac: mac)
        // 2. Wait for the voucher to approve.
        try await atlasWaitApproved(codeHash: codeHash)
        // 3. Redeem the bearer from the box over iroh (retry for allowlist propagation).
        let redeemed = try await linkRedeemOverIroh(reach: reach, seed: seed, code: code)

        if let bearer = redeemed.bearer, !bearer.isEmpty {
            try? KeychainStore.shared.saveBearer(bearer)
        }
        await MainActor.run {
            // `apiEndpoint` is only a path base over iroh (host ignored by the box).
            DeviceManager.shared.updateConfiguration(apiEndpoint: "http://virtues.box:8000")
            DeviceManager.shared.updateReach(boxNodeId: reach.boxNodeId, relayUrl: reach.relayUrl)
            DeviceManager.shared.updateActionIds(redeemed.actionIds ?? [:])
            DeviceManager.shared.isConfigured = true
            DeviceManager.shared.configurationState = .configured
        }
    }

    private func atlasLinkLookup(codeHash: String, endpointId: String, mac: String) async throws -> LinkLookupResponse {
        guard let url = URL(string: "\(Self.atlasBaseURL)/link/lookup") else { throw NetworkError.invalidURL }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = 15
        request.httpBody = try JSONSerialization.data(withJSONObject: [
            "code_hash": codeHash, "endpoint_id": endpointId, "mac": mac,
        ])
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw NetworkError.unknown(NSError(domain: "link lookup", code: 0))
        }
        guard http.statusCode == 200 else {
            throw NetworkError.badRequest(message: "Link code not found or expired. Ask for a fresh one.")
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(LinkLookupResponse.self, from: data)
    }

    /// Poll atlas until the voucher approves (or the code expires / we time out).
    private func atlasWaitApproved(codeHash: String) async throws {
        guard let url = URL(string: "\(Self.atlasBaseURL)/link/result") else { throw NetworkError.invalidURL }
        let body = try JSONSerialization.data(withJSONObject: ["code_hash": codeHash])
        // ~5 minutes at 2s intervals.
        for _ in 0..<150 {
            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = body
            request.timeoutInterval = 15
            if let (data, response) = try? await session.data(for: request),
               let http = response as? HTTPURLResponse, http.statusCode == 200,
               let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let status = obj["status"] as? String {
                switch status {
                case "approved": return
                case "expired": throw NetworkError.badRequest(message: "The link timed out. Start again.")
                default: break // pending / requested → keep waiting
                }
            }
            try await Task.sleep(nanoseconds: 2_000_000_000)
        }
        throw NetworkError.timeout
    }

    /// Dial the box over iroh (now allowlisted) and redeem the bearer. Retries a
    /// few times because allowlist registration propagates asynchronously.
    private func linkRedeemOverIroh(reach: LinkLookupResponse, seed: String, code: String) async throws -> LinkRedeemResponse {
        guard let url = URL(string: "http://virtues.box:8000/api/pair/link-redeem") else { throw NetworkError.invalidURL }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["code": code])
        let reqBytes = try HTTPWire.serialize(req)

        let transport = try await IrohTransport.dial(
            relayUrl: reach.relayUrl, boxIdHex: reach.boxNodeId, deviceSeedHex: seed, background: false
        )
        var lastError: Error?
        for attempt in 0..<6 {
            do {
                let respBytes = try await transport.request(rawHttp: reqBytes, background: false)
                let (data, http) = try HTTPWire.parseResponse(respBytes, url: url)
                if http.statusCode == 200 {
                    let decoder = JSONDecoder()
                    decoder.keyDecodingStrategy = .convertFromSnakeCase
                    return try decoder.decode(LinkRedeemResponse.self, from: data)
                }
                // 404 = not-approved-yet / propagation lag → retry.
                lastError = NetworkError.serverError(http.statusCode)
            } catch {
                lastError = error
            }
            if attempt < 5 { try await Task.sleep(nanoseconds: 2_000_000_000) }
        }
        throw lastError ?? NetworkError.timeout
    }

    // MARK: - CryptoKit helpers (must match box: SHA-256 / HMAC-SHA256, lowercase hex)

    private static func sha256Hex(_ s: String) -> String {
        SHA256.hash(data: Data(s.utf8)).map { String(format: "%02x", $0) }.joined()
    }
    private static func hmacSha256Hex(key: String, message: String) -> String {
        let mac = HMAC<SHA256>.authenticationCode(for: Data(message.utf8), using: SymmetricKey(data: Data(key.utf8)))
        return mac.map { String(format: "%02x", $0) }.joined()
    }

    // MARK: - Action runs (server-side outcome, 2B)

    /// Fetch recent server-side run history for one of this device's actions via
    /// `GET /api/devices/actions/{id}/runs` (device-bearer auth, ownership-scoped
    /// on the box). Goes through `BoxTransport`, so it tunnels like every other
    /// box call. On-demand only (StreamInfo view appear / refresh) — never in the
    /// background upload loop.
    func fetchActionRuns(actionId: String, limit: Int = 5) async throws -> [ActionRun] {
        guard let base = DeviceManager.shared.configuration.baseURL else {
            throw NetworkError.invalidURL
        }
        let token = DeviceManager.shared.configuration.deviceToken
        guard let url = URL(
            string: "\(base.absoluteString)/api/devices/actions/\(actionId)/runs?limit=\(limit)"
        ) else {
            throw NetworkError.invalidURL
        }

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.timeoutInterval = 15

        let (data, http) = try await BoxTransport.shared.send(request, session: session)
        guard http.statusCode == 200 else {
            if http.statusCode == 401 { throw NetworkError.invalidToken }
            throw NetworkError.serverError(http.statusCode)
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode([ActionRun].self, from: data)
    }

    // MARK: - iroh device identity

    /// Return this device's iroh seed (hex), generating + persisting a fresh
    /// 32-byte seed in the Keychain on first use so the EndpointId is stable
    /// across launches. `nil` only if the Keychain write fails.
    static func ensureIrohSeed() -> String? {
        if let existing = KeychainStore.shared.loadIrohSeed(), !existing.isEmpty {
            return existing
        }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            return nil
        }
        let hex = bytes.map { String(format: "%02x", $0) }.joined()
        try? KeychainStore.shared.saveIrohSeed(hex)
        return KeychainStore.shared.loadIrohSeed()
    }

    /// Register this device's iroh EndpointId with the box (provision path, where
    /// the device wasn't able to submit it in-band at consume). Authenticated with
    /// the device bearer; the box updates `app_device.node_id` for the caller.
    /// Best-effort — returns whether the box accepted it. Goes over `BoxTransport`
    /// (iroh), so the box must already reach this device's EndpointId; if it
    /// can't yet (freshly provisioned), the user re-pairs via the QR consume path.
    func registerSelfNodeId(base: URL, bearer: String, nodeId: String) async -> Bool {
        guard let url = URL(string: "\(base.absoluteString)/api/devices/self/node-id") else {
            return false
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        request.httpBody = try? JSONSerialization.data(withJSONObject: ["node_id": nodeId])
        request.timeoutInterval = 15
        guard let (_, http) = try? await BoxTransport.shared.send(request, session: session) else {
            return false
        }
        return (200...299).contains(http.statusCode)
    }

    /// Hardware model identifier (e.g. "iPhone16,1")
    private static var modelIdentifier: String {
        var systemInfo = utsname()
        uname(&systemInfo)
        let mirror = Mirror(reflecting: systemInfo.machine)
        return mirror.children.reduce("") { identifier, element in
            guard let value = element.value as? Int8, value != 0 else { return identifier }
            return identifier + String(UnicodeScalar(UInt8(value)))
        }
    }

    // MARK: - Connection Test

    func testConnection(endpoint: String) async -> Bool {
        guard let url = URL(string: endpoint) else { return false }
        
        var request = URLRequest(url: url)
        request.httpMethod = "HEAD"
        request.timeoutInterval = 5.0
        
        do {
            let (_, response) = try await session.data(for: request)
            if let httpResponse = response as? HTTPURLResponse {
                return (200...499).contains(httpResponse.statusCode)
            }
        } catch {
            // Log error but don't throw
            print("Connection test failed: \(error)")
        }
        
        return false
    }
}

// MARK: - Request/Response Models

/// Response from `POST /webhook/{action_id}` per
/// `virtues-core/src/server/webhook.rs::WebhookResponse`. The action runs async on
/// the server; `runId` is the row id in `app_action_runs` (null when status
/// is "skipped" or similar) and `status` is the dispatch outcome.
/// All fields are optional so the device tolerates server-side shape drift.
struct UploadResponse: Codable {
    let runId: String?
    let status: String?

    private enum CodingKeys: String, CodingKey {
        case runId = "run_id"
        case status
    }
}

struct ErrorResponse: Codable {
    let error: String
    let details: String?
    let message: String? // Added to match backend
}

// MARK: - Link-a-device (new-device side) responses

/// atlas `/link/lookup` → the box's public reach ticket.
struct LinkLookupResponse: Decodable {
    let boxNodeId: String
    let relayUrl: String
}

/// box `/api/pair/link-redeem` (over iroh) → the bearer + reach + action map.
struct LinkRedeemResponse: Decodable {
    let bearer: String?
    let credentialId: String?
    let actionIds: [String: String]?
    let boxNodeId: String?
    let relayUrl: String?
}

// MARK: - Pair-only flow models (v1)

/// Device-info JSON the box stores under `app_device.device_info` so the
/// `/virtues/devices` page can render a recognizable label. Plaintext,
/// non-secret context — never used for authentication.
struct PairingDeviceInfo: Codable {
    let device_id: String
    let device_name: String
    let device_model: String
    let os_version: String
    let app_version: String?
    /// IANA timezone of this device at pairing time (e.g. "America/Chicago").
    /// Used by the box as a cross-check for `home_timezone` when its own system
    /// clock reads UTC (cloud/datacenter deploys). See docs/timezone-model.md.
    let timezone: String?
}

/// `POST /api/pair/consume` body — see `virtues-core/src/api/pair.rs`
/// `ConsumeRequest`.
struct PairConsumeRequest: Codable {
    /// The pair token, 24 random bytes hex-encoded, lifted from the
    /// `/pair#t=...` fragment or `virtues://pair?t=...` deep link.
    let token: String
    /// "mobile_app" for the iOS app.
    let kind: String
    /// Optional human label shown on the box's Devices page. Defaults to
    /// the device's name (`UIDevice.current.name`) when nil.
    let label: String?
    let device_info: PairingDeviceInfo
    /// This device's iroh EndpointId (hex), derived from its locally-generated
    /// seed. The box allowlists it so the device can reach the box over iroh.
    let device_node_id: String?
    /// Idempotency key so a retried consume (lost response) re-returns the same
    /// bearer instead of burning the single-use token.
    let idempotency_key: String?
}

/// `POST /api/pair/consume` response — see `virtues-core/src/api/pair.rs`
/// `ConsumeResponse`. `actionIds` and `bearer` are the fields that matter
/// to the iOS app today; `bundle` is the future WG provisioning blob.
struct PairConsumeResponse: Codable {
    let deviceId: String
    let redirect: String
    /// Returned exactly once. Caller stores in Keychain; the in-memory
    /// copy on this struct should be discarded immediately after.
    let bearer: String?
    /// Backend `function_name → action_id` map the device persists and uses
    /// when posting each stream flush to `POST /webhook/{action_id}`.
    let actionIds: [String: String]
    /// The box's iroh EndpointId (hex) — the reach ticket the device dials over
    /// iroh. `nil` on a dev/LAN box with no relay reach.
    /// (`box_node_id` → `boxNodeId` via `.convertFromSnakeCase`.)
    let boxNodeId: String?
    /// The relay URL to reach `boxNodeId` through — the other half of the ticket.
    /// (`relay_url` → `relayUrl` via `.convertFromSnakeCase`.)
    let relayUrl: String?
}