//
//  DeviceConfiguration.swift
//  Virtues
//
//  Configuration model for device token and API settings
//

import Foundation
import UIKit

struct StreamConfig: Codable {
    let enabled: Bool
    let initialSyncDays: Int
    let displayName: String
}

/// Device configuration including API endpoint, device ID, and stream settings.
///
/// **Authentication Design:**
/// The device ID is used directly as the Bearer token for all API calls.
/// Users copy their device ID from the iOS app and enter it in the web app
/// to associate the device with their account.
struct DeviceConfiguration: Codable {
    let deviceId: String
    var apiEndpoint: String
    let deviceName: String
    var configuredDate: Date?
    var streamConfigurations: [String: StreamConfig] // Stream configurations from web app

    private enum CodingKeys: String, CodingKey {
        case deviceId = "device_id"
        case apiEndpoint = "api_endpoint"
        case deviceName = "device_name"
        case configuredDate = "configured_date"
        case streamConfigurations = "stream_configurations"
    }

    init(deviceId: String = UUID().uuidString,
         apiEndpoint: String = "",
         deviceName: String = UIDevice.current.name,
         configuredDate: Date? = nil,
         streamConfigurations: [String: StreamConfig] = [:]) {
        self.deviceId = deviceId
        self.apiEndpoint = apiEndpoint
        self.deviceName = deviceName
        self.configuredDate = configuredDate
        self.streamConfigurations = streamConfigurations
    }

    /// Device ID is used as the authentication token
    var deviceToken: String {
        deviceId
    }

    // Helper to check if device is configured (has a server URL)
    var isConfigured: Bool {
        return !apiEndpoint.isEmpty
    }
    
    // Helper to check if a specific stream is enabled
    func isStreamEnabled(_ streamKey: String) -> Bool {
        return streamConfigurations[streamKey]?.enabled ?? false
    }
    
    // Helper to get initial sync days for a stream
    func getInitialSyncDays(for streamKey: String) -> Int {
        return streamConfigurations[streamKey]?.initialSyncDays ?? 90
    }
    
    // Helper to get the full ingest URL
    var ingestURL: URL? {
        guard !apiEndpoint.isEmpty else { return nil }
        let cleanEndpoint = apiEndpoint.trimmingCharacters(in: .whitespacesAndNewlines)

        // Handle cases where user might have included /ingest already
        if cleanEndpoint.hasSuffix("/ingest") {
            return URL(string: cleanEndpoint)
        } else if cleanEndpoint.hasSuffix("/") {
            return URL(string: "\(cleanEndpoint)ingest")
        } else {
            return URL(string: "\(cleanEndpoint)/ingest")
        }
    }
}