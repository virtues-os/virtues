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
        let result = await saveWithRetry(metric: metric, deviceId: deviceId, maxAttempts: 3)

        switch result {
        case .success:
            print("✅ Saved battery metric to SQLite queue")
            dataUploader.updateUploadStats()
            return true
        case .failure(let error):
            ErrorLogger.shared.log(error, deviceId: deviceId)
            return false
        }
    }

    /// Attempts to save battery metric with exponential backoff retry
    private func saveWithRetry(metric: BatteryMetric, deviceId: String, maxAttempts: Int) async -> Result<Void, AnyDataCollectionError> {
        let streamData = BatteryStreamData(deviceId: deviceId, metrics: [metric])

        for attempt in 1...maxAttempts {
            // Encode the data
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601

            let data: Data
            do {
                data = try encoder.encode(streamData)
            } catch {
                let encodingError = DataEncodingError(
                    streamType: .battery,
                    underlyingError: error,
                    dataSize: nil
                )
                return .failure(AnyDataCollectionError(encodingError))
            }

            // Attempt to save to SQLite
            let success = storageProvider.enqueue(streamName: "ios_battery", data: data)

            if success {
                if attempt > 1 {
                    ErrorLogger.shared.logSuccessfulRetry(streamType: .battery, attemptNumber: attempt)
                }
                return .success(())
            }

            // If not last attempt, wait before retrying using async sleep (non-blocking)
            if attempt < maxAttempts {
                let delayNanoseconds = UInt64(Double(attempt) * 0.5 * 1_000_000_000)  // 0.5s, 1.0s backoff
                try? await Task.sleep(nanoseconds: delayNanoseconds)
            }
        }

        // All attempts failed
        let storageError = StorageError(
            streamType: .battery,
            reason: "Failed to enqueue to SQLite after \(maxAttempts) attempts",
            attemptNumber: maxAttempts
        )
        return .failure(AnyDataCollectionError(storageError))
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
    let source: String = "ios"
    let stream: String = "battery"
    let deviceId: String
    let records: [BatteryMetric]
    let timestamp: String
    let checkpoint: String?

    private enum CodingKeys: String, CodingKey {
        case source, stream
        case deviceId = "device_id"
        case records, timestamp, checkpoint
    }

    init(deviceId: String, metrics: [BatteryMetric], checkpoint: String? = nil) {
        self.deviceId = deviceId
        self.records = metrics
        self.timestamp = ISO8601DateFormatter().string(from: Date())
        self.checkpoint = checkpoint
    }
}
