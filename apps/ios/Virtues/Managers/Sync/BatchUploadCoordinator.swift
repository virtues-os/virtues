//
//  BatchUploadCoordinator.swift
//  Virtues
//
//  Coordinates batch uploads with retry logic and background handling
//

import Foundation
import UIKit
import BackgroundTasks
import Combine

class BatchUploadCoordinator: ObservableObject {
    static let shared = BatchUploadCoordinator()
    
    @Published var isUploading = false
    @Published var lastUploadDate: Date?
    @Published var lastSuccessfulSyncDate: Date?
    @Published var uploadStats: (pending: Int, failed: Int, total: Int) = (0, 0, 0)
    @Published var streamCounts: (healthkit: Int, location: Int, audio: Int) = (0, 0, 0)

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let networkManager: NetworkManager
    private let networkMonitor: NetworkMonitor

    private var uploadTimer: ReliableTimer?
    private let uploadInterval: TimeInterval = 300 // 5 minutes
    private let backgroundTaskIdentifier = "com.virtues.ios.sync"

    private var statsUpdateTimer: ReliableTimer?

    private let lastUploadKey = "com.virtues.lastUploadDate"
    private let lastSuccessfulSyncKey = "com.virtues.lastSuccessfulSyncDate"

    // MARK: - Circuit Breaker

    /// Consecutive upload failures (resets on success)
    private var consecutiveFailures = 0
    /// Timestamp of last failure (for circuit reset timeout)
    private var lastFailureTime: Date?
    /// Number of failures before circuit opens
    private let circuitBreakerThreshold = 10
    /// Time after which circuit resets (1 hour)
    private let circuitResetTimeout: TimeInterval = 3600

    /// Check if circuit breaker is open (too many failures)
    private var isCircuitOpen: Bool {
        guard consecutiveFailures >= circuitBreakerThreshold else { return false }
        guard let lastFailure = lastFailureTime else { return false }

        // Reset circuit after timeout
        if Date().timeIntervalSince(lastFailure) > circuitResetTimeout {
            consecutiveFailures = 0
            lastFailureTime = nil
            print("⚡ Circuit breaker reset after timeout")
            return false
        }
        return true
    }

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         networkManager: NetworkManager,
         networkMonitor: NetworkMonitor = NetworkMonitor.shared) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.networkManager = networkManager
        self.networkMonitor = networkMonitor

        loadLastUploadDate()
        loadLastSuccessfulSyncDate()
        registerBackgroundTasks()
        updateUploadStats()
        setupLowPowerModeObserver()
    }

    /// Legacy singleton initializer - uses default dependencies
    private convenience init() {
        self.init(
            configProvider: DeviceManager.shared,
            storageProvider: SQLiteManager.shared,
            networkManager: NetworkManager.shared
        )
    }
    
    // MARK: - Timer Management
    
    func startPeriodicUploads() {
        stopPeriodicUploads()

        // Schedule timer for 5-minute intervals
        uploadTimer = ReliableTimer.builder()
            .interval(uploadInterval)
            .qos(.userInitiated)
            .handler { [weak self] in
                Task {
                    await self?.performUpload()
                }
            }
            .build()

        // Schedule stats update timer for every 10 seconds (reduced from 2s for battery)
        statsUpdateTimer = ReliableTimer.builder()
            .interval(10.0)
            .qos(.utility)
            .handler { [weak self] in
                self?.updateUploadStats()
            }
            .build()

        // Perform initial upload
        Task {
            await performUpload()
        }

        // Schedule background task
        scheduleBackgroundUpload()
    }

    func stopPeriodicUploads() {
        uploadTimer?.cancel()
        uploadTimer = nil
        statsUpdateTimer?.cancel()
        statsUpdateTimer = nil
    }
    
    // MARK: - Upload Logic
    
    /// Performs upload and returns true if any uploads succeeded
    @discardableResult
    func performUpload() async -> Bool {
        #if DEBUG
        print("🚀 Starting upload process...")
        #endif

        // Check circuit breaker - stop retrying if too many consecutive failures
        if isCircuitOpen {
            #if DEBUG
            print("⚡ Circuit breaker OPEN - skipping upload (failures: \(consecutiveFailures), resets in \(Int(circuitResetTimeout - Date().timeIntervalSince(lastFailureTime ?? Date())))s)")
            #endif
            return false
        }

        // Check if Low Power Mode is enabled - pause uploads to save battery
        if ProcessInfo.processInfo.isLowPowerModeEnabled {
            #if DEBUG
            print("⚡️ Low Power Mode enabled - skipping upload to save battery")
            #endif
            return false
        }

        // Small delay to ensure any pending saves complete
        try? await Task.sleep(nanoseconds: 500_000_000) // 0.5 seconds

        // Reset stale uploads at the START of each sync cycle
        // This catches records stuck in "uploading" for >10 minutes (interrupted syncs)
        _ = storageProvider.resetStaleUploads()

        // Clean up any bad events first
        _ = storageProvider.cleanupBadEvents()

        guard configProvider.isConfigured else {
            return false
        }

        guard let ingestURL = configProvider.ingestURL else {
            return false
        }
        
        await MainActor.run {
            self.isUploading = true
        }
        
        defer {
            Task { @MainActor [weak self] in
                guard let self = self else { return }
                self.isUploading = false
                self.updateUploadStats()
            }
        }
        
        // Get pending uploads with adaptive batch size based on network conditions
        let batchSize = networkMonitor.getRecommendedBatchSize()
        let pendingEvents = storageProvider.dequeueNext(limit: batchSize)

        #if DEBUG
        print("Using adaptive batch size: \(batchSize) events (network: \(networkMonitor.currentNetworkType.description))")
        #endif
        
        if pendingEvents.isEmpty {
            return true  // No events to upload is not a failure
        }
        
        // Group events by stream name
        let groupedEvents = Dictionary(grouping: pendingEvents) { $0.streamName }
        
        // Track if any uploads succeeded
        var anyUploadSucceeded = false

        // Process each stream group as a batch
        for (streamName, events) in groupedEvents {
            let success = await uploadBatchedEvents(streamName: streamName, events: events, to: ingestURL)

            // Record upload result for adaptive batching
            networkMonitor.recordUploadResult(success: success)

            if success {
                anyUploadSucceeded = true
            }
        }
        
        // Update last upload date (attempt)
        saveLastUploadDate(Date())
        
        // Update last successful sync date if any uploads succeeded
        if anyUploadSucceeded {
            saveLastSuccessfulSyncDate(Date())
        }
        
        // Cleanup old events
        _ = storageProvider.cleanupOldEvents()

        return anyUploadSucceeded
    }

    private func uploadBatchedEvents(streamName: String, events: [UploadEvent], to url: URL) async -> Bool {
        // Get the appropriate processor for this stream type
        guard let processor = StreamProcessorFactory.processor(for: streamName) else {
            print("⚠️ No processor found for stream: \(streamName)")
            for event in events {
                storageProvider.incrementRetry(id: event.id)
            }
            return false
        }

        // Use type-erased processing based on stream type
        switch streamName {
        case "ios_healthkit":
            let healthKitProcessor = processor as! HealthKitStreamProcessor
            return await uploadWithProcessor(processor: healthKitProcessor, events: events, to: url)

        case "ios_location":
            let locationProcessor = processor as! LocationStreamProcessor
            return await uploadWithProcessor(processor: locationProcessor, events: events, to: url)

        case "ios_mic":
            let audioProcessor = processor as! AudioStreamProcessor
            return await uploadWithProcessor(processor: audioProcessor, events: events, to: url)

        default:
            for event in events {
                storageProvider.incrementRetry(id: event.id)
            }
            return false
        }
    }

    /// Generic upload method that works with any stream processor
    private func uploadWithProcessor<P: StreamDataProcessor>(processor: P, events: [UploadEvent], to url: URL) async -> Bool {
        do {
            var allItems: [P.DataType] = []

            // Decode each event and collect all items
            for event in events {
                do {
                    let items = try processor.decode(event.dataBlob)
                    allItems.append(contentsOf: items)
                } catch {
                    print("⚠️ Failed to decode event \(event.id): \(error)")
                    storageProvider.incrementRetry(id: event.id)
                }
            }

            // If we have items, combine and upload
            guard !allItems.isEmpty else {
                return false
            }

            let combinedData = processor.combine(allItems, deviceId: configProvider.deviceId)

            // Upload combined data
            let response = try await networkManager.uploadData(
                combinedData,
                deviceToken: configProvider.deviceToken,
                endpoint: url
            )

            // Mark all events as complete
            for event in events {
                handleSuccessfulUpload(event: event, response: response)
            }

            return true

        } catch {
            for event in events {
                handleFailedUpload(event: event, error: error)
            }
            return false
        }
    }

    private func handleSuccessfulUpload(event: UploadEvent, response: UploadResponse) {
        // Mark as complete in SQLite
        storageProvider.markAsComplete(id: event.id)

        // Reset circuit breaker on success
        consecutiveFailures = 0
        lastFailureTime = nil
    }

    private func handleFailedUpload(event: UploadEvent, error: Error) {
        if let networkError = error as? NetworkError {
            switch networkError {
            case .rateLimited(let retryAfter):
                // Rate limited - back off but don't break circuit
                // The event stays pending for retry after the delay
                storageProvider.incrementRetry(id: event.id)
                #if DEBUG
                print("⚠️ Rate limited - retry after \(Int(retryAfter))s (NOT counting toward circuit breaker)")
                #endif
                return

            case .badRequest(let message):
                // Permanent failure - don't retry
                storageProvider.markAsFailed(id: event.id)
                #if DEBUG
                print("❌ Bad request (permanent failure): \(message)")
                #endif
                return

            case .forbidden:
                // Permanent failure - don't retry
                storageProvider.markAsFailed(id: event.id)
                #if DEBUG
                print("❌ Forbidden (permanent failure)")
                #endif
                return

            case .invalidToken:
                // Auth failure - stop all uploads until re-auth
                storageProvider.incrementRetry(id: event.id)
                stopPeriodicUploads()
                #if DEBUG
                print("❌ Invalid token - stopping uploads until re-auth")
                #endif
                return

            case .noConnection:
                // Transient - retry when online, don't count toward circuit breaker
                storageProvider.incrementRetry(id: event.id)
                #if DEBUG
                print("⚠️ No connection - will retry when online")
                #endif
                return

            case .serverError, .timeout:
                // Transient server issues - retry with backoff, count toward circuit breaker
                storageProvider.incrementRetry(id: event.id)
                consecutiveFailures += 1
                lastFailureTime = Date()

                #if DEBUG
                print("⚠️ Server error/timeout - failures: \(consecutiveFailures)/\(circuitBreakerThreshold)")
                if consecutiveFailures >= circuitBreakerThreshold {
                    print("⚡ Circuit breaker OPENED after \(consecutiveFailures) consecutive failures")
                }
                #endif

            case .invalidURL, .decodingError, .unknown:
                // Other errors - increment retry but don't count toward circuit breaker
                storageProvider.incrementRetry(id: event.id)
                #if DEBUG
                print("⚠️ Other error: \(networkError.localizedDescription)")
                #endif
            }
        } else {
            // Non-network error - increment retry, count toward circuit breaker
            storageProvider.incrementRetry(id: event.id)
            consecutiveFailures += 1
            lastFailureTime = Date()

            #if DEBUG
            print("⚠️ Unknown error: \(error.localizedDescription)")
            #endif
        }
    }
    
    // MARK: - Background Tasks
    
    private func registerBackgroundTasks() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: backgroundTaskIdentifier, using: nil) { task in
            self.handleBackgroundTask(task: task as! BGProcessingTask)
        }
    }
    
    private func scheduleBackgroundUpload() {
        let request = BGProcessingTaskRequest(identifier: backgroundTaskIdentifier)
        request.requiresNetworkConnectivity = true
        request.requiresExternalPower = false
        request.earliestBeginDate = Date(timeIntervalSinceNow: uploadInterval)
        
        do {
            try BGTaskScheduler.shared.submit(request)
            print("Background upload task scheduled")
        } catch {
            print("Failed to schedule background task: \(error)")
        }
    }
    
    private func handleBackgroundTask(task: BGProcessingTask) {
        // Schedule next background task
        scheduleBackgroundUpload()

        // Create a task to perform upload
        let uploadTask = Task { () -> Bool in
            await performUpload()
        }

        // Set expiration handler
        task.expirationHandler = {
            uploadTask.cancel()
        }

        // Notify completion when done with actual result
        Task {
            let result = await uploadTask.result
            switch result {
            case .success(let uploadSucceeded):
                task.setTaskCompleted(success: uploadSucceeded)
            case .failure:
                task.setTaskCompleted(success: false)
            }
        }
    }
    
    // MARK: - Manual Upload
    
    func triggerManualUpload() async {
        await performUpload()
    }
    
    // MARK: - Statistics
    
    func updateUploadStats() {
        let stats = storageProvider.getQueueStats()
        let counts = storageProvider.getStreamCounts()

        Task { @MainActor [weak self] in
            guard let self = self else { return }
            self.uploadStats = (stats.pending, stats.failed, stats.total)
            self.streamCounts = counts
        }
    }

    func getQueueSizeString() -> String {
        let stats = storageProvider.getQueueStats()
        let bytes = stats.totalSize
        
        if bytes < 1024 {
            return "\(bytes) B"
        } else if bytes < 1024 * 1024 {
            return String(format: "%.1f KB", Double(bytes) / 1024.0)
        } else {
            return String(format: "%.1f MB", Double(bytes) / (1024.0 * 1024.0))
        }
    }
    
    // MARK: - Persistence
    
    private func loadLastUploadDate() {
        if let timestamp = UserDefaults.standard.object(forKey: lastUploadKey) as? TimeInterval {
            let date = Date(timeIntervalSince1970: timestamp)
            Task { @MainActor [weak self] in
                self?.lastUploadDate = date
            }
        }
    }

    private func loadLastSuccessfulSyncDate() {
        if let timestamp = UserDefaults.standard.object(forKey: lastSuccessfulSyncKey) as? TimeInterval {
            let date = Date(timeIntervalSince1970: timestamp)
            Task { @MainActor [weak self] in
                self?.lastSuccessfulSyncDate = date
            }
        }
    }

    private func saveLastUploadDate(_ date: Date) {
        Task { @MainActor [weak self] in
            self?.lastUploadDate = date
        }
        UserDefaults.standard.set(date.timeIntervalSince1970, forKey: lastUploadKey)
    }

    private func saveLastSuccessfulSyncDate(_ date: Date) {
        Task { @MainActor [weak self] in
            self?.lastSuccessfulSyncDate = date
        }
        UserDefaults.standard.set(date.timeIntervalSince1970, forKey: lastSuccessfulSyncKey)
    }
    
    // MARK: - Low Power Mode Monitoring

    private func setupLowPowerModeObserver() {
        // Observe Low Power Mode changes
        NotificationCenter.default.addObserver(
            forName: Notification.Name.NSProcessInfoPowerStateDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            let isLowPowerModeEnabled = ProcessInfo.processInfo.isLowPowerModeEnabled

            #if DEBUG
            print("⚡️ Low Power Mode changed: \(isLowPowerModeEnabled ? "ENABLED" : "DISABLED")")
            #endif

            // When Low Power Mode is disabled, trigger immediate upload
            if !isLowPowerModeEnabled, let self = self {
                #if DEBUG
                print("⚡️ Low Power Mode disabled - triggering queued uploads")
                #endif

                Task {
                    await self.performUpload()
                }
            }
        }
    }

    // MARK: - Network Monitoring

    func handleNetworkChange(isConnected: Bool) {
        if isConnected && configProvider.isConfigured {
            // Network restored, trigger upload
            Task {
                await performUpload()
            }
        }
    }
}
