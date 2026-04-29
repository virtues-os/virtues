//
//  DeviceConfiguration.swift
//  Virtues
//
//  Configuration model for device token and API settings
//

import Foundation
import UIKit

/// Device configuration including API endpoint, device ID, auth token, and
/// per-stream action_ids.
///
/// **Authentication Design:**
/// The device ID is used directly as the Bearer token for all API calls.
/// Users pair their device via the web app to associate it with their account.
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

    private enum CodingKeys: String, CodingKey {
        case deviceId = "device_id"
        case apiEndpoint = "api_endpoint"
        case deviceName = "device_name"
        case configuredDate = "configured_date"
        case actionIds = "action_ids"
    }

    init(deviceId: String = UUID().uuidString,
         apiEndpoint: String = "",
         deviceName: String = UIDevice.current.name,
         configuredDate: Date? = nil,
         actionIds: [String: String] = [:]) {
        self.deviceId = deviceId
        self.apiEndpoint = apiEndpoint
        self.deviceName = deviceName
        self.configuredDate = configuredDate
        self.actionIds = actionIds
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.deviceId = try c.decode(String.self, forKey: .deviceId)
        self.apiEndpoint = try c.decode(String.self, forKey: .apiEndpoint)
        self.deviceName = try c.decode(String.self, forKey: .deviceName)
        self.configuredDate = try c.decodeIfPresent(Date.self, forKey: .configuredDate)
        self.actionIds = (try? c.decodeIfPresent([String: String].self, forKey: .actionIds)) ?? [:]
    }

    /// Device ID is used as the authentication token
    var deviceToken: String {
        deviceId
    }

    // Helper to check if device is configured (has a server URL)
    var isConfigured: Bool {
        return !apiEndpoint.isEmpty
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

    /// Get the webhook URL for a given stream name. Returns nil if the device
    /// hasn't been repaired since the webhook cutover (actionIds is empty or
    /// missing an entry for this stream). The caller should refetch via
    /// `GET /api/devices/action-ids` and retry.
    func webhookURL(forStream streamName: String) -> URL? {
        let canonical = DeviceConfiguration.canonicalStreamName(streamName)
        guard let actionId = actionIds[canonical] else { return nil }
        guard let base = baseURL else { return nil }
        return base.appendingPathComponent("webhook").appendingPathComponent(actionId)
    }

    /// URL for refetching the action_ids map via device-token auth.
    var actionIdsFetchURL: URL? {
        baseURL?.appendingPathComponent("api/devices/action-ids")
    }
}
