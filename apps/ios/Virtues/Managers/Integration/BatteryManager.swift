//
//  BatteryManager.swift
//  Virtues
//
//  Monitors device battery level and charging state
//

import Foundation
import UIKit
import Combine

/// Thread-safe battery monitoring manager.
/// @MainActor ensures all @Published property updates occur on the main thread,
/// preventing race conditions when collectBatteryData() updates batteryLevel/batteryState.
@MainActor
class BatteryManager: ObservableObject {
    static let shared = BatteryManager()

    @Published var isMonitoring = false
    @Published var batteryLevel: Float = 0
    @Published var batteryState: UIDevice.BatteryState = .unknown

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader

    private var batteryTimer: ReliableTimer?
    private let lastSyncKey = "com.virtues.battery.lastSync"

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         dataUploader: DataUploader) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.dataUploader = dataUploader

        // Enable battery monitoring
        UIDevice.current.isBatteryMonitoringEnabled = true
        // Direct update in init is safe since @MainActor class runs init on main thread
        batteryLevel = UIDevice.current.batteryLevel
        batteryState = UIDevice.current.batteryState
    }

    /// Legacy singleton initializer - uses default dependencies
    private convenience init() {
        self.init(
            configProvider: DeviceManager.shared,
            storageProvider: SQLiteManager.shared,
            dataUploader: BatchUploadCoordinator.shared
        )
    }

    // MARK: - Monitoring Control

    func startMonitoring() {
        guard !isMonitoring else { return }

        print("🔋 Starting battery monitoring")

        // Ensure battery monitoring is enabled
        UIDevice.current.isBatteryMonitoringEnabled = true

        // Stop any existing timer
        stopMonitoring()

        // Start 5-minute timer
        batteryTimer = ReliableTimer.builder()
            .interval(300.0)  // 5 minutes
            .qos(.utility)
            .handler { [weak self] in
                Task { @MainActor in
                    await self?.collectBatteryData()
                }
            }
            .build()

        isMonitoring = true

        // Collect immediately
        Task { @MainActor in
            await self.collectBatteryData()
        }

        print("🔋 Battery monitoring started with 5-minute intervals")
    }

    func stopMonitoring() {
        if let timer = batteryTimer {
            print("🔋 Stopping battery timer")
            timer.cancel()
            batteryTimer = nil
        }

        // Since BatteryManager is @MainActor, we're already on main thread
        // Setting synchronously avoids race condition with startMonitoring()
        isMonitoring = false
        print("🔋 Battery monitoring stopped")
    }

    // MARK: - Data Collection

    private func updateBatteryInfo() async {
        // Ensure @Published property updates occur on main thread
        await MainActor.run {
            batteryLevel = UIDevice.current.batteryLevel
            batteryState = UIDevice.current.batteryState
        }
    }

    private func collectBatteryData() async {
        await updateBatteryInfo()

        let metric = BatteryMetric(
            timestamp: Date(),
            level: batteryLevel,
            state: stateString(for: batteryState),
            isLowPowerMode: ProcessInfo.processInfo.isLowPowerModeEnabled
        )

        print("🔋 Collected battery data: \(Int(batteryLevel * 100))% (\(metric.state))")

        // Save to SQLite queue
        let success = await saveMetricToQueue(metric)
        if success {
            UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: lastSyncKey)
        }
    }

    private func stateString(for state: UIDevice.BatteryState) -> String {
        switch state {
        case .charging: return "charging"
        case .full: return "full"
        case .unplugged: return "unplugged"
        case .unknown: return "unknown"
        @unknown default: return "unknown"
        }
    }

    // MARK: - Data Persistence

    private func saveMetricToQueue(_ metric: BatteryMetric) async -> Bool {
        let deviceId = configProvider.deviceId

        let streamData = BatteryStreamData(
            deviceId: deviceId,
            metrics: [metric]
        )

        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601

        guard let data = try? encoder.encode(streamData) else {
            print("❌ Failed to encode battery metric")
            return false
        }

        let success = storageProvider.enqueue(streamName: "ios_battery", data: data)

        if success {
            print("✅ Saved battery metric to SQLite queue")
            dataUploader.updateUploadStats()
        } else {
            print("❌ Failed to save battery metric to SQLite")
        }

        return success
    }
}

// MARK: - Data Models

struct BatteryMetric: Codable {
    let timestamp: Date
    let level: Float
    let state: String
    let isLowPowerMode: Bool
}

struct BatteryStreamData: Codable {
    let deviceId: String
    let metrics: [BatteryMetric]
}
