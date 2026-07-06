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

    /// Backend function_name → action_id map. Empty for devices that haven't
    /// been paired (or re-paired) since the webhook unification.
    var actionIds: [String: String] { get }

    /// Get the webhook URL for a given stream name. Returns nil if no
    /// action_id is known for that stream (caller should refetch via
    /// `GET /api/devices/action-ids`).
    func webhookURL(forStream streamName: String) -> URL?

    /// URL for refetching the action_ids map.
    var actionIdsFetchURL: URL? { get }
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

    var actionIds: [String: String] {
        configuration.actionIds
    }

    func webhookURL(forStream streamName: String) -> URL? {
        configuration.webhookURL(forStream: streamName)
    }

    var actionIdsFetchURL: URL? {
        configuration.actionIdsFetchURL
    }
}
