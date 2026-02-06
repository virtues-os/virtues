//
//  ConfigurationProvider.swift
//  Virtues
//
//  Protocol for providing device configuration to managers
//  Enables dependency injection and testing
//

import Foundation

/// Provides device configuration
protocol ConfigurationProvider {
    /// Unique device identifier
    var deviceId: String { get }

    /// Whether the device is fully configured
    var isConfigured: Bool { get }

    /// The API endpoint URL
    var apiEndpoint: String { get }

    /// The device token for authentication
    var deviceToken: String { get }

    /// The full ingest URL for uploads
    var ingestURL: URL? { get }
}

/// Provides observable configuration updates for SwiftUI views
protocol ObservableConfigurationProvider: ConfigurationProvider, ObservableObject {
    /// Current configuration state
    var configurationState: DeviceConfigurationState { get }
}

// MARK: - DeviceConfiguration Extension

extension DeviceConfiguration: ConfigurationProvider {
    // Already implements all required properties through the struct
}

// MARK: - DeviceManager Extension

extension DeviceManager: ObservableConfigurationProvider {
    // deviceId and deviceToken are already defined in DeviceManager

    var apiEndpoint: String {
        configuration.apiEndpoint
    }

    var ingestURL: URL? {
        configuration.ingestURL
    }
}
