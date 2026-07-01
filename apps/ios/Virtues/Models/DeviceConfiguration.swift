//
//  DeviceConfiguration.swift
//  Virtues
//
//  Configuration model for device token and API settings
//

import Foundation
import UIKit

/// Device configuration including API endpoint, device ID, and per-stream
/// action_ids.
///
/// **Authentication Design (v1, pair-only):**
/// The bearer token is **server-issued** at pair time and stored in the OS
/// Keychain via `KeychainStore.shared`. The `deviceId` on this struct is a
/// non-secret label only — it identifies which device this is in the box's
/// `/virtues/devices` UI, but it has zero authentication weight.
///
/// The legacy `deviceToken: String { deviceId }` accessor was removed in v1.
/// Callers use `KeychainStore.shared.loadBearer()` to read the bearer.
///
/// **Webhook routing:**
/// Each ingest stream (healthkit, location, microphone, etc.) has its own
/// backend `app_actions` row with a stable `action_id`. The server returns
/// a `function_name → action_id` map at pair time; the device stores this and
/// routes each stream flush to `POST /webhook/{action_id}`.
struct DeviceConfiguration: Codable {
    let deviceId: String
    var apiEndpoint: String
    let deviceName: String
    var configuredDate: Date?
    /// Backend function_name → action_id. Populated at pair time and after a
    /// call to `GET /api/devices/action-ids`. Empty for legacy configs that
    /// predate the webhook unification; `webhookURL(forStream:)` returns nil
    /// in that case and the caller should refetch.
    var actionIds: [String: String]
    /// The box's iroh EndpointId (hex) — half the reach ticket the app dials over
    /// iroh. `nil` on a dev/LAN box with no relay reach. Non-secret.
    var boxNodeId: String?
    /// The relay URL to reach `boxNodeId` through — the other half of the ticket.
    var relayUrl: String?

    private enum CodingKeys: String, CodingKey {
        case deviceId = "device_id"
        case apiEndpoint = "api_endpoint"
        case deviceName = "device_name"
        case configuredDate = "configured_date"
        case actionIds = "action_ids"
        case boxNodeId = "box_node_id"
        case relayUrl = "relay_url"
    }

    init(deviceId: String = UUID().uuidString,
         apiEndpoint: String = "",
         deviceName: String = UIDevice.current.name,
         configuredDate: Date? = nil,
         actionIds: [String: String] = [:],
         boxNodeId: String? = nil,
         relayUrl: String? = nil) {
        self.deviceId = deviceId
        self.apiEndpoint = apiEndpoint
        self.deviceName = deviceName
        self.configuredDate = configuredDate
        self.actionIds = actionIds
        self.boxNodeId = boxNodeId
        self.relayUrl = relayUrl
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.deviceId = try c.decode(String.self, forKey: .deviceId)
        self.apiEndpoint = try c.decode(String.self, forKey: .apiEndpoint)
        self.deviceName = try c.decode(String.self, forKey: .deviceName)
        self.configuredDate = try c.decodeIfPresent(Date.self, forKey: .configuredDate)
        self.actionIds = (try? c.decodeIfPresent([String: String].self, forKey: .actionIds)) ?? [:]
        self.boxNodeId = try? c.decodeIfPresent(String.self, forKey: .boxNodeId)
        self.relayUrl = try? c.decodeIfPresent(String.self, forKey: .relayUrl)
    }

    /// Bearer token read from the Keychain. Set by the pair flow; returns
    /// `nil` if the device hasn't been paired (or was revoked + wiped).
    var bearerToken: String? {
        if let kc = KeychainStore.shared.loadBearer(), !kc.isEmpty {
            return kc
        }
        return nil
    }

    /// `Authorization: Bearer <token>` value used on every box API call.
    /// Required by the `ConfigurationProvider` protocol (callers expect a
    /// `String`, not `String?`).
    ///
    /// The legacy `deviceId`-as-bearer fallback was retired in v1.1: pairing now
    /// always provisions a real bearer (and a WG bundle) in the Keychain. If no
    /// bearer is present the device simply isn't paired — we return an empty
    /// string so the box rejects the call (401) and the UI prompts a re-pair,
    /// rather than silently sending a non-credential the box never honored.
    var deviceToken: String {
        bearerToken ?? ""
    }

    /// True when this device has both an endpoint AND a usable bearer
    /// (Keychain or legacy). The onboarding gate uses this to decide
    /// whether to allow data collection to begin.
    var isConfigured: Bool {
        !apiEndpoint.isEmpty
    }

    /// True when the user has set an endpoint but hasn't completed a pair
    /// via the v1 pair-only flow (no Keychain bearer). The UI uses this to
    /// show "pair to finish setup" rather than "you're paired."
    var awaitingPair: Bool {
        !apiEndpoint.isEmpty && bearerToken == nil
    }

    /// Terse iOS internal stream names → canonical backend function_names.
    /// The terse names predate the webhook unification and are baked into
    /// manager code + persisted SQLite rows, so rather than rename them in
    /// a dozen places we alias here at the edge.
    private static let streamNameAliases: [String: String] = [
        "ios_mic": "ios_microphone",
        "ios_finance": "ios_financekit",
    ]

    /// Canonicalize a stream name for action_id lookup.
    static func canonicalStreamName(_ name: String) -> String {
        streamNameAliases[name] ?? name
    }

    /// Get the base URL (without any path). Matches the pairing flow — if
    /// the user included a trailing `/api`, strip it so routes compose
    /// cleanly.
    var baseURL: URL? {
        guard !apiEndpoint.isEmpty else { return nil }
        var clean = apiEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        if clean.hasSuffix("/") { clean = String(clean.dropLast()) }
        if clean.hasSuffix("/api") { clean = String(clean.dropLast(4)) }
        return URL(string: clean)
    }

    /// Get the webhook URL for a stream. Every iOS stream now posts to the one
    /// `ios_ingest` action — the backend fans out by the `stream` field in the
    /// request body — so `streamName` no longer selects the URL; it's kept in
    /// the signature because callers group uploads by stream. Returns nil if the
    /// device hasn't paired since the ingest unification (no `ios_ingest` entry
    /// in `actionIds`); the caller should refetch via
    /// `GET /api/devices/action-ids` and retry.
    func webhookURL(forStream streamName: String) -> URL? {
        guard let actionId = actionIds["ios_ingest"] else { return nil }
        guard let base = baseURL else { return nil }
        return base.appendingPathComponent("webhook").appendingPathComponent(actionId)
    }

    /// URL for refetching the action_ids map via device-token auth.
    var actionIdsFetchURL: URL? {
        baseURL?.appendingPathComponent("api/devices/action-ids")
    }
}
