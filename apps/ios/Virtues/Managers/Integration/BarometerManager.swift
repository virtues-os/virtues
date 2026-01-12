//
//  BarometerManager.swift
//  Virtues
//
//  Monitors barometric pressure and relative altitude using CMAltimeter
//  No permission required - uses on-device sensors
//

import Foundation
import CoreMotion
import Combine

/// Thread-safe barometer monitoring manager.
/// Uses CMAltimeter to capture pressure and altitude changes.
@MainActor
class BarometerManager: ObservableObject {
    static let shared = BarometerManager()

    @Published var isMonitoring = false
    @Published var currentPressure: Double? = nil  // kPa
    @Published var relativeAltitude: Double? = nil  // meters

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader

    private let altimeter = CMAltimeter()
    private var collectionTimer: ReliableTimer?
    private var pendingMetrics: [BarometerMetric] = []
    private let lastSyncKey = "com.virtues.barometer.lastSync"

    /// Check if barometer is available on this device
    static var isAvailable: Bool {
        CMAltimeter.isRelativeAltitudeAvailable()
    }

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         dataUploader: DataUploader) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.dataUploader = dataUploader
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
        guard BarometerManager.isAvailable else {
            print("📊 Barometer not available on this device")
            return
        }

        print("📊 Starting barometer monitoring")

        // Start altimeter updates
        altimeter.startRelativeAltitudeUpdates(to: .main) { [weak self] data, error in
            guard let self = self, let data = data else {
                if let error = error {
                    print("📊 Altimeter error: \(error.localizedDescription)")
                }
                return
            }

            Task { @MainActor in
                self.currentPressure = data.pressure.doubleValue  // kPa
                self.relativeAltitude = data.relativeAltitude.doubleValue  // meters

                // Create metric for this reading
                let metric = BarometerMetric(
                    timestamp: Date(),
                    pressureKPa: data.pressure.doubleValue,
                    relativeAltitudeMeters: data.relativeAltitude.doubleValue
                )
                self.pendingMetrics.append(metric)
            }
        }

        // Start timer to batch and save metrics every 5 minutes
        collectionTimer = ReliableTimer.builder()
            .interval(300.0)  // 5 minutes
            .qos(.utility)
            .handler { [weak self] in
                Task { @MainActor in
                    await self?.flushMetrics()
                }
            }
            .build()

        isMonitoring = true
        print("📊 Barometer monitoring started")
    }

    func stopMonitoring() {
        altimeter.stopRelativeAltitudeUpdates()

        if let timer = collectionTimer {
            timer.cancel()
            collectionTimer = nil
        }

        // Flush any remaining metrics
        Task { @MainActor in
            await flushMetrics()
        }

        isMonitoring = false
        print("📊 Barometer monitoring stopped")
    }

    // MARK: - Data Collection

    private func flushMetrics() async {
        guard !pendingMetrics.isEmpty else { return }

        let metricsToSave = pendingMetrics
        pendingMetrics = []

        print("📊 Flushing \(metricsToSave.count) barometer metrics")

        let success = await saveMetricsToQueue(metricsToSave)
        if success {
            UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: lastSyncKey)
        }
    }

    // MARK: - Data Persistence

    private func saveMetricsToQueue(_ metrics: [BarometerMetric]) async -> Bool {
        let deviceId = configProvider.deviceId

        let streamData = BarometerStreamData(
            deviceId: deviceId,
            metrics: metrics
        )

        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601

        guard let data = try? encoder.encode(streamData) else {
            print("   Failed to encode barometer metrics")
            return false
        }

        let success = storageProvider.enqueue(streamName: "ios_barometer", data: data)

        if success {
            print("   Saved barometer metrics to SQLite queue")
            dataUploader.updateUploadStats()
        } else {
            print("   Failed to save barometer metrics to SQLite")
        }

        return success
    }
}

// MARK: - Data Models

struct BarometerMetric: Codable {
    let timestamp: Date
    let pressureKPa: Double
    let relativeAltitudeMeters: Double
}

struct BarometerStreamData: Codable {
    let deviceId: String
    let metrics: [BarometerMetric]
}
