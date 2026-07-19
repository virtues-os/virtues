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
/// **Authentication Design (iroh-key, no bearer):**
/// This device authenticates by its own iroh key — a 32-byte seed generated at
/// pairing and stored in the OS Keychain (`KeychainStore.shared.irohSeed`). Its
/// EndpointId is submitted to the box, which allowlists it; every request over
/// `BoxTransport` (iroh) is then authenticated by that proven key. There is no
/// bearer token. `deviceId` is a non-secret label for the box's Devices page.
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
    /// `nil` on an unclaimed box (reached purely by `boxDirectAddrs`).
    var relayUrl: String?
    /// The box's direct iroh sockets (`IP:port`, e.g. a LAN `192.168.x:51820`)
    /// from the pairing reach ticket. Dialed by NodeId with nobody in the loop —
    /// this is how an unclaimed box is reached LAN-direct (a Tailscale socket is
    /// derived from `apiEndpoint` at dial time; see `DeviceManager`).
    var boxDirectAddrs: [String]?

    private enum CodingKeys: String, CodingKey {
        case deviceId = "device_id"
        case apiEndpoint = "api_endpoint"
        case deviceName = "device_name"
        case configuredDate = "configured_date"
        case actionIds = "action_ids"
        case boxNodeId = "box_node_id"
        case relayUrl = "relay_url"
        case boxDirectAddrs = "box_direct_addrs"
    }

    init(deviceId: String = UUID().uuidString,
         apiEndpoint: String = "",
         deviceName: String = UIDevice.current.name,
         configuredDate: Date? = nil,
         actionIds: [String: String] = [:],
         boxNodeId: String? = nil,
         relayUrl: String? = nil,
         boxDirectAddrs: [String]? = nil) {
        self.deviceId = deviceId
        self.apiEndpoint = apiEndpoint
        self.deviceName = deviceName
        self.configuredDate = configuredDate
        self.actionIds = actionIds
        self.boxNodeId = boxNodeId
        self.relayUrl = relayUrl
        self.boxDirectAddrs = boxDirectAddrs
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
        self.boxDirectAddrs = try? c.decodeIfPresent([String].self, forKey: .boxDirectAddrs)
    }

    /// True when this device has an endpoint. The onboarding gate uses this to
    /// decide whether to allow data collection to begin.
    var isConfigured: Bool {
        !apiEndpoint.isEmpty
    }

    /// True when the user has set an endpoint but hasn't completed a pair. Auth
    /// is the device's iroh key, so "paired" = an iroh seed exists in the
    /// Keychain; without one the UI shows "pair to finish setup".
    var awaitingPair: Bool {
        !apiEndpoint.isEmpty && KeychainStore.shared.loadIrohSeed() == nil
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
