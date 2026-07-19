//
//  DeviceManager.swift
//  Virtues
//
//  Manages device configuration and authentication state
//

import Foundation
import UIKit
import Combine
import Network

enum DeviceConfigurationState {
    case notConfigured
    case configured           // Server URL is set, device ID is used as auth
}

/// The everything-needed-to-dial-the-box triple, resolved from persisted state:
/// the box's EndpointId + relay URL (from `DeviceConfiguration`) and this
/// device's iroh seed (from the Keychain). Read off any thread by `BoxTransport`.
struct IrohTicket {
    let boxNodeId: String
    /// `nil` on an unclaimed box (no relay) — reached via `directAddrs`.
    let relayUrl: String?
    /// Box direct sockets (`IP:port`) to dial by NodeId: the box's reported LAN
    /// addrs plus a derived `<paired-host>:iroh-port` (covers Tailscale).
    let directAddrs: [String]
    let deviceSeed: String
}

class DeviceManager: ObservableObject {
    static let shared = DeviceManager()
    
    @Published var configuration: DeviceConfiguration
    @Published var isConfigured: Bool = false
    @Published var configurationState: DeviceConfigurationState = .notConfigured
    @Published var isVerifying: Bool = false
    @Published var lastError: String?
    @Published var updateRequired: Bool = false
    private let userDefaults = UserDefaults.standard
    /// UserDefaults key for the persisted `DeviceConfiguration`. Static so the
    /// thread-safe `currentReachTicket()` reader can decode it without touching
    /// the `@Published` in-memory copy.
    static let configKey = "com.virtues.deviceConfiguration"

    private var cancellables = Set<AnyCancellable>()
    
    private init() {
        // Load saved configuration or create new one
        if let savedData = userDefaults.data(forKey: Self.configKey),
           let savedConfig = try? JSONDecoder().decode(DeviceConfiguration.self, from: savedData) {
            self.configuration = savedConfig
            self.isConfigured = savedConfig.isConfigured
        } else {
            self.configuration = DeviceConfiguration()
            self.isConfigured = false
        }
        
        // Observe configuration changes to save automatically
        $configuration
            .debounce(for: .milliseconds(500), scheduler: RunLoop.main)
            .sink { [weak self] config in
                self?.saveConfiguration(config)
                self?.isConfigured = config.isConfigured
            }
            .store(in: &cancellables)
    }
    
    // MARK: - Configuration Management

    /// The device ID is used as the authentication token for all API calls.
    /// User copies this ID to the web app to associate the device with their account.
    var deviceId: String {
        configuration.deviceId
    }

    func updateConfiguration(apiEndpoint: String) {
        configuration.apiEndpoint = apiEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        configuration.configuredDate = Date()

        // Save configuration to UserDefaults
        saveConfiguration(configuration)
    }

    /// The box's pinned iroh UDP port (matches the box default `VIRTUES_IROH_PORT`
    /// in `virtues-iroh`). Used to derive a direct dial socket from the paired
    /// endpoint host (e.g. a Tailscale `100.x` → `100.x:51820`).
    static let irohPort = 51820

    /// Persist the box's iroh reach ticket (`box_node_id` + optional `relay_url` +
    /// its direct sockets) from a pair/consume response. Non-secret; the device
    /// seed lives in the Keychain. Also drops `BoxTransport`'s warm connection so
    /// the next call redials with the new ticket.
    func updateReach(boxNodeId: String?, relayUrl: String?, boxDirectAddrs: [String]? = nil) {
        Task { @MainActor in
            self.configuration.boxNodeId = (boxNodeId?.isEmpty == false) ? boxNodeId : nil
            self.configuration.relayUrl = (relayUrl?.isEmpty == false) ? relayUrl : nil
            if let addrs = boxDirectAddrs, !addrs.isEmpty {
                self.configuration.boxDirectAddrs = addrs
            }
            self.saveConfiguration(self.configuration)
            // Reset the warm transport AFTER the new ticket is persisted, so a
            // concurrent send() can't redial off the old ticket in between.
            await BoxTransport.shared.reset()
        }
    }

    /// Resolve the full dial ticket from persisted state, readable off any thread
    /// (UserDefaults + Keychain are thread-safe). Requires the box EndpointId +
    /// this device's seed, and at least one reach path: a relay (claimed box) OR a
    /// direct socket (unclaimed box — LAN addr from the ticket, and/or a derived
    /// `<paired-host>:iroh-port` that carries Tailscale).
    static func currentReachTicket() -> IrohTicket? {
        guard
            let data = UserDefaults.standard.data(forKey: Self.configKey),
            let cfg = try? JSONDecoder().decode(DeviceConfiguration.self, from: data),
            let node = cfg.boxNodeId, !node.isEmpty,
            let seed = KeychainStore.shared.loadIrohSeed(), !seed.isEmpty
        else { return nil }
        let relay = (cfg.relayUrl?.isEmpty == false) ? cfg.relayUrl : nil

        var direct = cfg.boxDirectAddrs ?? []
        // Derive `<paired-host>:iroh-port` from the endpoint we paired against,
        // but ONLY when the host is a numeric IP literal (Tailscale `100.x` / a
        // LAN IP) — that's what the Rust FFI can `parse::<SocketAddr>()`. A
        // hostname would be silently dropped there, so counting it here would let
        // the guard below hand back a ticket that fails at dial time with a
        // generic error; skip it and rely on the box's reported LAN addrs / relay.
        // Bracket IPv6 so `<ip>:port` parses, matching iroh's `SocketAddr` form.
        if let host = URL(string: cfg.apiEndpoint)?.host {
            let derived: String?
            if IPv4Address(host) != nil {
                derived = "\(host):\(Self.irohPort)"
            } else if IPv6Address(host) != nil {
                derived = "[\(host)]:\(Self.irohPort)"
            } else {
                derived = nil
            }
            if let derived, !direct.contains(derived) { direct.append(derived) }
        }

        guard relay != nil || !direct.isEmpty else { return nil }
        return IrohTicket(boxNodeId: node, relayUrl: relay, directAddrs: direct, deviceSeed: seed)
    }

    /// Replace the stored `function_name → action_id` map. Called after a
    /// successful pair or after a refetch from `/api/devices/action-ids`.
    func updateActionIds(_ actionIds: [String: String]) {
        Task { @MainActor in
            self.configuration.actionIds = actionIds
            self.saveConfiguration(self.configuration)
        }
    }
    
    private func saveConfiguration(_ config: DeviceConfiguration) {
        if let encoded = try? JSONEncoder().encode(config) {
            userDefaults.set(encoded, forKey: Self.configKey)
        } else {
            print("❌ Failed to encode configuration for saving")
        }
    }
    
    func clearConfiguration() {
        // Keep the same deviceId when clearing (it's the device's permanent identifier)
        let existingDeviceId = configuration.deviceId
        configuration = DeviceConfiguration(deviceId: existingDeviceId)
        userDefaults.removeObject(forKey: Self.configKey)

        isConfigured = false
        configurationState = .notConfigured
        lastError = nil
    }
    
    // MARK: - Validation
    
    func validateEndpoint(_ endpoint: String) -> Bool {
        let trimmed = endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        
        // Basic URL validation
        if trimmed.isEmpty { return false }
        
        // Check if it's a valid URL
        if let url = URL(string: trimmed) {
            // Allow http for local development
            return url.scheme == "http" || url.scheme == "https"
        }
        
        return false
    }
    
    // MARK: - Minimum Version Gate

    /// Check if this app version meets the server's minimum requirement.
    /// Called on launch and periodically during sync cycles.
    func checkMinimumVersion() async {
        guard isConfigured, !configuration.apiEndpoint.isEmpty else { return }

        do {
            guard let url = URL(string: "\(configuration.apiEndpoint)/health") else { return }
            // Through BoxTransport so it tunnels (works off-LAN, never plaintext)
            // for a paired device, consistent with every other box call.
            let request = URLRequest(url: url)
            let (data, http) = try await BoxTransport.shared.send(request, session: .shared)
            guard http.statusCode == 200 else { return }

            struct HealthResponse: Decodable {
                let min_ios_version: String?
            }

            let health = try JSONDecoder().decode(HealthResponse.self, from: data)
            guard let minVersion = health.min_ios_version else { return }

            let currentVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0"

            let needsUpdate = compareVersions(current: currentVersion, minimum: minVersion)
            await MainActor.run {
                self.updateRequired = needsUpdate
            }
        } catch {
            // Network error - don't block the user, will check again next cycle
        }
    }

    /// Returns true if current version is older than minimum.
    private func compareVersions(current: String, minimum: String) -> Bool {
        let currentParts = current.split(separator: ".").compactMap { Int($0) }
        let minimumParts = minimum.split(separator: ".").compactMap { Int($0) }

        for i in 0..<max(currentParts.count, minimumParts.count) {
            let c = i < currentParts.count ? currentParts[i] : 0
            let m = i < minimumParts.count ? minimumParts[i] : 0
            if c < m { return true }
            if c > m { return false }
        }
        return false // Equal versions
    }

    // MARK: - Debug Helpers

    func getDebugInfo() -> String {
        var info = "Device Configuration:\n"
        info += "- Device ID: \(configuration.deviceId)\n"
        info += "- Configured: \(isConfigured)\n"
        info += "- Endpoint: \(configuration.apiEndpoint.isEmpty ? "Not set" : configuration.apiEndpoint)\n"

        return info
    }
}