//
//  DeviceManager.swift
//  Virtues
//
//  Manages device configuration and authentication state
//

import Foundation
import UIKit
import Combine

enum DeviceConfigurationState {
    case notConfigured
    case configured           // Server URL is set, device ID is used as auth
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
    private let configKey = "com.virtues.deviceConfiguration"
    
    private var cancellables = Set<AnyCancellable>()
    
    private init() {
        // Load saved configuration or create new one
        if let savedData = userDefaults.data(forKey: configKey),
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

    /// Alias for deviceId - used as Bearer token for API authentication
    var deviceToken: String {
        deviceId
    }

    func updateConfiguration(apiEndpoint: String) {
        configuration.apiEndpoint = apiEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        configuration.configuredDate = Date()

        // Save configuration to UserDefaults
        saveConfiguration(configuration)
    }
    
    func updateEndpoint(_ newEndpoint: String) async -> Bool {
        let trimmedEndpoint = newEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        
        // Validate the endpoint format
        guard validateEndpoint(trimmedEndpoint) else {
            await MainActor.run {
                self.lastError = "Invalid endpoint URL format"
            }
            return false
        }
        
        // Test the connection to the new endpoint
        let isReachable = await NetworkManager.shared.testConnection(endpoint: trimmedEndpoint)
        if !isReachable {
            await MainActor.run {
                self.lastError = "Cannot reach the new endpoint"
            }
            return false
        }
        
        // Update the configuration
        await MainActor.run {
            self.configuration.apiEndpoint = trimmedEndpoint
            self.lastError = nil
            
            // Force save the configuration
            self.saveConfiguration(self.configuration)
        }

        return true
    }

    private func saveConfiguration(_ config: DeviceConfiguration) {
        if let encoded = try? JSONEncoder().encode(config) {
            userDefaults.set(encoded, forKey: configKey)
        } else {
            print("❌ Failed to encode configuration for saving")
        }
    }
    
    func clearConfiguration() {
        // Keep the same deviceId when clearing (it's the device's permanent identifier)
        let existingDeviceId = configuration.deviceId
        configuration = DeviceConfiguration(deviceId: existingDeviceId)
        userDefaults.removeObject(forKey: configKey)

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
            let (data, response) = try await URLSession.shared.data(from: url)
            guard let http = response as? HTTPURLResponse, http.statusCode == 200 else { return }

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