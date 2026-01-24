//
//  HealthKitManager.swift
//  Virtues
//
//  Manages HealthKit authorization and data collection
//

import Foundation
import HealthKit
import Combine

class HealthKitManager: ObservableObject {
    static let shared = HealthKitManager()

    private let healthStore = HKHealthStore()

    @Published var isAuthorized = false
    @Published var isMonitoring = false
    @Published var authorizationStatus: [String: Bool] = [:]
    @Published var lastSyncDate: Date?
    @Published var isSyncing = false

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader

    private let lastSyncKey = "com.virtues.healthkit.lastSync"
    private let hasRequestedAuthKey = "com.virtues.healthkit.hasRequestedAuth"
    private var healthTimer: ReliableTimer?
    private var hasRequestedAuthorization: Bool {
        get { UserDefaults.standard.bool(forKey: hasRequestedAuthKey) }
        set { UserDefaults.standard.set(newValue, forKey: hasRequestedAuthKey) }
    }

    var hasRequestedHealthKitAuthorization: Bool {
        hasRequestedAuthorization
    }

    // Anchors for incremental sync
    var anchors: [String: HKQueryAnchor] = [:]
    private let anchorKeyPrefix = "com.virtues.healthkit.anchor."

    // Define all HealthKit types we need
    private let healthKitTypes: Set<HKSampleType> = [
        HKQuantityType.quantityType(forIdentifier: .heartRate)!,
        HKQuantityType.quantityType(forIdentifier: .stepCount)!,
        HKQuantityType.quantityType(forIdentifier: .activeEnergyBurned)!,
        HKQuantityType.quantityType(forIdentifier: .heartRateVariabilitySDNN)!,
        HKQuantityType.quantityType(forIdentifier: .distanceWalkingRunning)!,
        HKQuantityType.quantityType(forIdentifier: .restingHeartRate)!,
        HKCategoryType.categoryType(forIdentifier: .sleepAnalysis)!
    ]

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         dataUploader: DataUploader) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.dataUploader = dataUploader

        loadLastSyncDate()
        loadAnchors()
        checkAuthorizationStatus()

        // Register with centralized health check coordinator
        HealthCheckCoordinator.shared.register(self)
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
        print("🏥 startMonitoring called, isAuthorized: \(isAuthorized)")

        guard isAuthorized else {
            print("❌ HealthKit not authorized, cannot start monitoring")
            return
        }

        // Stop any existing timer
        stopMonitoring()

        // Start the 5-minute timer (aligned with sync interval)
        print("⏱️ Creating HealthKit timer with 5-minute interval")
        healthTimer = ReliableTimer.builder()
            .interval(300.0)  // 5 minutes
            .qos(.userInitiated)
            .handler { [weak self] in
                print("⏰ HealthKit timer fired - collecting data...")
                Task {
                    await self?.collectNewData()
                }
            }
            .build()
        
        // Fire immediately to start collecting
        print("🚀 Triggering immediate HealthKit data collection")
        Task {
            await collectNewData()
        }

        isMonitoring = true
        print("🏥 Started HealthKit monitoring with 5-minute intervals")
    }

    func stopMonitoring() {
        if let timer = healthTimer {
            print("🛑 Cancelling HealthKit timer")
            timer.cancel()
            healthTimer = nil
        }

        isMonitoring = false
        print("🛑 Stopped HealthKit monitoring")
    }

    /// Manual sync trigger for UI "Force Sync" button
    func performSync() async {
        await collectNewData()
    }

    // MARK: - Authorization
    
    func requestAuthorization() async -> Bool {
        guard HKHealthStore.isHealthDataAvailable() else {
            print("HealthKit is not available on this device")
            return false
        }

        do {
            try await healthStore.requestAuthorization(toShare: [], read: healthKitTypes)

            // Mark that we've requested authorization
            hasRequestedAuthorization = true

            // After requesting, test if we actually have access
            let hasAccess = await testHealthKitAccess()

            await MainActor.run {
                self.isAuthorized = hasAccess
            }

            return hasAccess
        } catch {
            print("HealthKit authorization request failed: \(error)")
            return false
        }
    }
    
    func checkAuthorizationStatus() {
        // Check authorization status for all HealthKit types we need
        print("🏥 Checking HealthKit authorization status...")
        Task {
            let hasAccess = await testHealthKitAccess()
            print("🏥 HealthKit access test result: \(hasAccess)")
            await MainActor.run {
                self.isAuthorized = hasAccess
            }
        }
    }

    func hasAllPermissions() -> Bool {
        return isAuthorized
    }

    // Test if we can actually read HealthKit data
    // NOTE: Due to provisioning profile issues, authorizationStatus() may lie and return .sharingDenied
    // even when the user has granted permission (visible in Settings). This method tests ACTUAL access
    // by attempting to query data, which is more reliable than trusting the status API.
    private func testHealthKitAccess() async -> Bool {
        // First, check if we've even requested authorization
        if !hasRequestedAuthorization {
            print("🏥 Result: NOT GRANTED (never requested)")
            return false
        }

        print("🏥 Testing HealthKit access by attempting actual data query...")

        // Try to query a small amount of step count data
        // This tests actual permission, not what the API claims
        guard let stepType = HKQuantityType.quantityType(forIdentifier: .stepCount) else {
            print("🏥 Result: ERROR (couldn't create step count type)")
            return false
        }

        let endDate = Date()
        let startDate = Calendar.current.date(byAdding: .day, value: -1, to: endDate)!
        let predicate = HKQuery.predicateForSamples(withStart: startDate, end: endDate, options: .strictStartDate)

        // Use async/await instead of semaphore to avoid blocking the main thread
        return await withCheckedContinuation { continuation in
            let query = HKSampleQuery(sampleType: stepType, predicate: predicate, limit: 1, sortDescriptors: nil) { _, samples, error in
                if let error = error as NSError? {
                    // Check if error is permission-related
                    if error.domain == "com.apple.healthkit" && error.code == 5 { // HKError.errorAuthorizationDenied
                        print("🏥 Actual query test: DENIED (permission error: \(error.localizedDescription))")
                        continuation.resume(returning: false)
                    } else {
                        // Other errors mean we have permission but something else failed
                        print("🏥 Actual query test: GRANTED (non-permission error: \(error.localizedDescription))")
                        continuation.resume(returning: true)
                    }
                } else {
                    // No error = we have permission (even if samples is empty/nil)
                    let sampleCount = samples?.count ?? 0
                    print("🏥 Actual query test: GRANTED (query succeeded, returned \(sampleCount) samples)")
                    continuation.resume(returning: true)
                }
            }
            self.healthStore.execute(query)
        }
    }
    
    // MARK: - Initial Sync
    
    func performInitialSync(progressHandler: @escaping (Double) -> Void) async -> Bool {
        guard isAuthorized else {
            print("❌ HealthKit not authorized for initial sync")
            return false
        }
        
        await MainActor.run {
            self.isSyncing = true
        }
        
        defer {
            Task { @MainActor in
                self.isSyncing = false
            }
        }
        
        // Full history sync: go back 10 years in yearly chunks
        let yearsToSync = 10
        let now = Date()
        var allSuccess = true
        
        print("🏁 Starting HealthKit full history sync for \(yearsToSync) years")
        
        for yearOffset in 0..<yearsToSync {
            let chunkEndDate = Calendar.current.date(byAdding: .year, value: -yearOffset, to: now)!
            let chunkStartDate = Calendar.current.date(byAdding: .year, value: -1, to: chunkEndDate)!
            
            print("📅 Syncing HealthKit chunk: \(chunkStartDate) to \(chunkEndDate)")
            
            var chunkMetrics: [HealthKitMetric] = []
            
            for type in healthKitTypes {
                if let metrics = await collectData(for: type, from: chunkStartDate, to: chunkEndDate) {
                    chunkMetrics.append(contentsOf: metrics)
                }
            }
            
            if !chunkMetrics.isEmpty {
                print("📦 Collected \(chunkMetrics.count) metrics for chunk. Saving...")
                let success = await saveMetricsToQueue(chunkMetrics)
                if !success {
                    allSuccess = false
                }
            }
            
            // Update progress
            let progress = Double(yearOffset + 1) / Double(yearsToSync)
            await MainActor.run {
                progressHandler(progress)
            }
        }
        
        if allSuccess {
            saveLastSyncDate(now)
            
            // Capture current anchors for future incremental syncs
            print("📍 Capturing anchors for incremental sync...")
            for type in healthKitTypes {
                if let newAnchor = await captureCurrentAnchor(for: type) {
                    let typeKey = getAnchorKey(for: type)
                    anchors[typeKey] = newAnchor
                    saveAnchor(newAnchor, for: typeKey)
                }
            }
        }
        
        return allSuccess
    }
    
    
    // MARK: - Data Collection
    
    private func collectData(for type: HKSampleType, from startDate: Date, to endDate: Date) async -> [HealthKitMetric]? {
        let predicate = HKQuery.predicateForSamples(withStart: startDate, end: endDate, options: .strictStartDate)
        
        return await withCheckedContinuation { continuation in
            if let quantityType = type as? HKQuantityType {
                collectQuantityData(type: quantityType, predicate: predicate) { metrics in
                    continuation.resume(returning: metrics)
                }
            } else if let categoryType = type as? HKCategoryType {
                collectCategoryData(type: categoryType, predicate: predicate) { metrics in
                    continuation.resume(returning: metrics)
                }
            } else {
                continuation.resume(returning: nil)
            }
        }
    }
    
    private func collectNewData(for type: HKSampleType, anchor: HKQueryAnchor?) async -> ([HealthKitMetric], HKQueryAnchor?)? {
        // First, fetch the raw samples using a Sendable-safe closure
        let queryResult: ([HKSample], HKQueryAnchor?)? = await withCheckedContinuation { continuation in
            let query = HKAnchoredObjectQuery(
                type: type,
                predicate: nil, // Get all new samples
                anchor: anchor,
                limit: HKObjectQueryNoLimit
            ) { _, samplesOrNil, _, newAnchor, error in
                // This closure doesn't capture self, avoiding Sendable issues
                guard let samples = samplesOrNil, error == nil else {
                    if let error = error {
                        print("❌ HealthKit query error for \(type.identifier): \(error)")
                    }
                    continuation.resume(returning: nil)
                    return
                }
                continuation.resume(returning: (samples, newAnchor))
            }
            healthStore.execute(query)
        }

        // Process samples outside the closure where we can safely use self
        guard let (samples, newAnchor) = queryResult else {
            return nil
        }

        var metrics: [HealthKitMetric] = []

        // Process quantity samples
        if let quantitySamples = samples as? [HKQuantitySample],
           let quantityType = type as? HKQuantityType {
            metrics = quantitySamples.compactMap { sample in
                let metricType = self.getMetricType(for: quantityType)
                let unit = self.getUnit(for: quantityType)
                let value = self.getValue(from: sample, type: quantityType).roundedForHealthKit(metricType: metricType)

                var metadata: [String: Any] = [:]
                if quantityType.identifier == HKQuantityType.quantityType(forIdentifier: .heartRate)!.identifier {
                    metadata["activity_context"] = self.getActivityContext(from: sample)
                }

                return HealthKitMetric(
                    timestamp: sample.startDate,
                    metricType: metricType,
                    value: value,
                    unit: unit,
                    metadata: metadata.isEmpty ? nil : metadata
                )
            }
        }

        // Process category samples
        else if let categorySamples = samples as? [HKCategorySample],
                let categoryType = type as? HKCategoryType {
            metrics = categorySamples.map { sample in
                let metricType = self.getMetricType(for: categoryType)
                let value = Double(sample.value)

                var metadata: [String: Any] = [:]
                if categoryType.identifier == HKCategoryType.categoryType(forIdentifier: .sleepAnalysis)!.identifier {
                    metadata["sleep_state"] = self.getSleepState(from: sample.value)
                    metadata["duration_minutes"] = Int(sample.endDate.timeIntervalSince(sample.startDate) / 60)
                }

                return HealthKitMetric(
                    timestamp: sample.startDate,
                    metricType: metricType,
                    value: value,
                    unit: "category",
                    metadata: metadata.isEmpty ? nil : metadata
                )
            }
        }

        return (metrics, newAnchor)
    }
    
    private func collectQuantityData(type: HKQuantityType, predicate: NSPredicate, completion: @escaping ([HealthKitMetric]) -> Void) {
        let query = HKSampleQuery(sampleType: type, predicate: predicate, limit: HKObjectQueryNoLimit, sortDescriptors: nil) { _, samples, error in
            guard let samples = samples as? [HKQuantitySample], error == nil else {
                completion([])
                return
            }
            
            let metrics = samples.map { sample -> HealthKitMetric in
                let metricType = self.getMetricType(for: type)
                let unit = self.getUnit(for: type)
                let value = self.getValue(from: sample, type: type).roundedForHealthKit(metricType: metricType)
                
                var metadata: [String: Any] = [:]
                
                // Add metadata based on type
                if type.identifier == HKQuantityType.quantityType(forIdentifier: .heartRate)!.identifier {
                    metadata["activity_context"] = self.getActivityContext(from: sample)
                }
                
                return HealthKitMetric(
                    timestamp: sample.startDate,
                    metricType: metricType,
                    value: value,
                    unit: unit,
                    metadata: metadata.isEmpty ? nil : metadata
                )
            }
            
            completion(metrics)
        }
        
        healthStore.execute(query)
    }
    
    private func collectCategoryData(type: HKCategoryType, predicate: NSPredicate, completion: @escaping ([HealthKitMetric]) -> Void) {
        let query = HKSampleQuery(sampleType: type, predicate: predicate, limit: HKObjectQueryNoLimit, sortDescriptors: nil) { _, samples, error in
            guard let samples = samples as? [HKCategorySample], error == nil else {
                completion([])
                return
            }
            
            let metrics = samples.map { sample -> HealthKitMetric in
                let metricType = self.getMetricType(for: type)
                let value = Double(sample.value)
                
                var metadata: [String: Any] = [:]
                
                // Add sleep-specific metadata
                if type.identifier == HKCategoryType.categoryType(forIdentifier: .sleepAnalysis)!.identifier {
                    metadata["sleep_state"] = self.getSleepState(from: sample.value)
                    metadata["duration_minutes"] = Int(sample.endDate.timeIntervalSince(sample.startDate) / 60)
                }
                
                return HealthKitMetric(
                    timestamp: sample.startDate,
                    metricType: metricType,
                    value: value,
                    unit: "category",
                    metadata: metadata
                )
            }
            
            completion(metrics)
        }
        
        healthStore.execute(query)
    }
    
    // MARK: - Helper Methods
    
    private func getTypeString(for type: HKSampleType) -> String {
        if let quantityType = type as? HKQuantityType {
            return quantityType.identifier
        } else if let categoryType = type as? HKCategoryType {
            return categoryType.identifier
        }
        return "unknown"
    }
    
    private func getMetricType(for type: HKSampleType) -> String {
        let identifier = getTypeString(for: type)
        
        switch identifier {
        case HKQuantityType.quantityType(forIdentifier: .heartRate)!.identifier:
            return "heart_rate"
        case HKQuantityType.quantityType(forIdentifier: .stepCount)!.identifier:
            return "steps"
        case HKQuantityType.quantityType(forIdentifier: .activeEnergyBurned)!.identifier:
            return "active_energy"
        case HKQuantityType.quantityType(forIdentifier: .heartRateVariabilitySDNN)!.identifier:
            return "heart_rate_variability"
        case HKQuantityType.quantityType(forIdentifier: .distanceWalkingRunning)!.identifier:
            return "distance"
        case HKQuantityType.quantityType(forIdentifier: .restingHeartRate)!.identifier:
            return "resting_heart_rate"
        case HKCategoryType.categoryType(forIdentifier: .sleepAnalysis)!.identifier:
            return "sleep"
        default:
            return "unknown"
        }
    }
    
    private func getUnit(for type: HKQuantityType) -> String {
        switch type.identifier {
        case HKQuantityType.quantityType(forIdentifier: .heartRate)!.identifier,
             HKQuantityType.quantityType(forIdentifier: .restingHeartRate)!.identifier:
            return "bpm"
        case HKQuantityType.quantityType(forIdentifier: .stepCount)!.identifier:
            return "steps"
        case HKQuantityType.quantityType(forIdentifier: .activeEnergyBurned)!.identifier:
            return "kcal"
        case HKQuantityType.quantityType(forIdentifier: .heartRateVariabilitySDNN)!.identifier:
            return "ms"
        case HKQuantityType.quantityType(forIdentifier: .distanceWalkingRunning)!.identifier:
            return "m"
        default:
            return "unknown"
        }
    }
    
    private func getValue(from sample: HKQuantitySample, type: HKQuantityType) -> Double {
        switch type.identifier {
        case HKQuantityType.quantityType(forIdentifier: .heartRate)!.identifier,
             HKQuantityType.quantityType(forIdentifier: .restingHeartRate)!.identifier:
            return sample.quantity.doubleValue(for: HKUnit.count().unitDivided(by: .minute()))
        case HKQuantityType.quantityType(forIdentifier: .stepCount)!.identifier:
            return sample.quantity.doubleValue(for: .count())
        case HKQuantityType.quantityType(forIdentifier: .activeEnergyBurned)!.identifier:
            return sample.quantity.doubleValue(for: .kilocalorie())
        case HKQuantityType.quantityType(forIdentifier: .heartRateVariabilitySDNN)!.identifier:
            return sample.quantity.doubleValue(for: .secondUnit(with: .milli))
        case HKQuantityType.quantityType(forIdentifier: .distanceWalkingRunning)!.identifier:
            return sample.quantity.doubleValue(for: .meter())
        default:
            return 0
        }
    }
    
    private func getActivityContext(from sample: HKQuantitySample) -> String {
        // Check metadata for motion context
        if let metadata = sample.metadata,
           let context = metadata[HKMetadataKeyHeartRateMotionContext] as? NSNumber {
            switch context.intValue {
            case 1: return "resting"
            case 2: return "active"
            default: return "unknown"
            }
        }
        return "unknown"
    }
    
    private func getSleepState(from value: Int) -> String {
        switch value {
        case HKCategoryValueSleepAnalysis.inBed.rawValue:
            return "in_bed"
        case HKCategoryValueSleepAnalysis.asleepUnspecified.rawValue:
            return "asleep"
        case HKCategoryValueSleepAnalysis.awake.rawValue:
            return "awake"
        case HKCategoryValueSleepAnalysis.asleepCore.rawValue:
            return "asleep_core"
        case HKCategoryValueSleepAnalysis.asleepDeep.rawValue:
            return "asleep_deep"
        case HKCategoryValueSleepAnalysis.asleepREM.rawValue:
            return "asleep_rem"
        default:
            // Log the unknown value for debugging
            print("⚠️ Unknown sleep state value: \(value)")
            return "unknown"
        }
    }
    
    // MARK: - Buffered Data Collection
    
    private func collectNewData() async {
        print("🏥 HealthKit timer fired - collecting new data...")
        
        var allMetrics: [HealthKitMetric] = []
        
        // Collect new data for each type using anchored queries
        for type in healthKitTypes {
            let typeKey = getAnchorKey(for: type)
            let anchor = anchors[typeKey]
            
            if let (metrics, newAnchor) = await collectNewData(for: type, anchor: anchor) {
                if !metrics.isEmpty {
                    print("🏥 Found \(metrics.count) new \(type.identifier) samples")
                    allMetrics.append(contentsOf: metrics)
                }
                
                // Update anchor for next query
                if let newAnchor = newAnchor {
                    anchors[typeKey] = newAnchor
                    saveAnchor(newAnchor, for: typeKey)
                }
            }
        }
        
        // Save to SQLite in batches if needed
        if !allMetrics.isEmpty {
            print("🏥 Collected \(allMetrics.count) new health metrics")
            
            // For regular syncs, batch if more than 1000 metrics
            if allMetrics.count > 1000 {
                let batchSize = 1000
                let totalBatches = (allMetrics.count + batchSize - 1) / batchSize
                var allSuccess = true
                
                print("🏥 Saving \(allMetrics.count) metrics in \(totalBatches) batches")
                
                for batchIndex in 0..<totalBatches {
                    let startIndex = batchIndex * batchSize
                    let endIndex = min((batchIndex + 1) * batchSize, allMetrics.count)
                    let batch = Array(allMetrics[startIndex..<endIndex])
                    
                    let success = await saveMetricsToQueue(batch)
                    if !success {
                        allSuccess = false
                    }
                }
                
                if allSuccess {
                    await MainActor.run {
                        self.lastSyncDate = Date()
                    }
                }
            } else {
                // Small enough to save as single batch
                let success = await saveMetricsToQueue(allMetrics)
                if success {
                    await MainActor.run {
                        self.lastSyncDate = Date()
                    }
                }
            }
        } else {
            print("🏥 No new health metrics found")
        }
    }
    
    
    // MARK: - Data Persistence
    
    private func saveMetricsToQueue(_ metrics: [HealthKitMetric]) async -> Bool {
        let deviceId = configProvider.deviceId

        // Attempt to save with retry mechanism
        let result = await saveWithRetry(metrics: metrics, deviceId: deviceId, maxAttempts: 3)

        switch result {
        case .success:
            print("✅ Saved \(metrics.count) HealthKit metrics to SQLite queue")
            dataUploader.updateUploadStats()
            return true

        case .failure(let error):
            ErrorLogger.shared.log(error, deviceId: deviceId)
            return false
        }
    }

    /// Attempts to save HealthKit metrics with exponential backoff retry
    private func saveWithRetry(metrics: [HealthKitMetric], deviceId: String, maxAttempts: Int) async -> Result<Void, AnyDataCollectionError> {
        let streamData = HealthKitStreamData(
            deviceId: deviceId,
            metrics: metrics
        )

        for attempt in 1...maxAttempts {
            // Encode the data
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601

            let data: Data
            do {
                data = try encoder.encode(streamData)
            } catch {
                let encodingError = DataEncodingError(
                    streamType: .healthKit,
                    underlyingError: error,
                    dataSize: metrics.count
                )
                return .failure(AnyDataCollectionError(encodingError))
            }

            // Attempt to save to SQLite
            let success = storageProvider.enqueue(streamName: "ios_healthkit", data: data)

            if success {
                if attempt > 1 {
                    ErrorLogger.shared.logSuccessfulRetry(streamType: .healthKit, attemptNumber: attempt)
                }
                return .success
            }

            // If not last attempt, wait before retrying
            if attempt < maxAttempts {
                let delay = Double(attempt) * 0.5  // 0.5s, 1.0s backoff
                try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            }
        }

        // All attempts failed
        let storageError = StorageError(
            streamType: .healthKit,
            reason: "Failed to enqueue to SQLite after \(maxAttempts) attempts",
            attemptNumber: maxAttempts
        )
        return .failure(AnyDataCollectionError(storageError))
    }
    
    private func loadLastSyncDate() {
        if let timestamp = UserDefaults.standard.object(forKey: lastSyncKey) as? TimeInterval {
            lastSyncDate = Date(timeIntervalSince1970: timestamp)
        }
    }
    
    private func saveLastSyncDate(_ date: Date) {
        Task { @MainActor in
            lastSyncDate = date
        }
        UserDefaults.standard.set(date.timeIntervalSince1970, forKey: lastSyncKey)
    }
    
    // MARK: - Anchor Management
    
    private func getAnchorKey(for type: HKSampleType) -> String {
        return anchorKeyPrefix + type.identifier
    }
    
    private func loadAnchors() {
        for type in healthKitTypes {
            let key = getAnchorKey(for: type)
            if let anchorData = UserDefaults.standard.data(forKey: key),
               let anchor = try? NSKeyedUnarchiver.unarchivedObject(ofClass: HKQueryAnchor.self, from: anchorData) {
                anchors[key] = anchor
            }
        }
    }
    
    private func saveAnchor(_ anchor: HKQueryAnchor, for key: String) {
        if let anchorData = try? NSKeyedArchiver.archivedData(withRootObject: anchor, requiringSecureCoding: true) {
            UserDefaults.standard.set(anchorData, forKey: key)
        }
    }

    /// Capture the current anchor for a sample type by performing a quick anchored query.
    /// This is used after initial sync to establish a baseline for incremental syncs.
    ///
    /// **Why limit=0?**
    /// Apple's HealthKit API requires running an actual anchored query to get a valid anchor.
    /// HKQueryAnchor cannot be created directly from arbitrary values (HKQueryAnchor(fromValue:)
    /// with Int.max would skip all existing samples). Using limit=0 executes a minimal query
    /// that returns the current anchor position without fetching any data.
    private func captureCurrentAnchor(for type: HKSampleType) async -> HKQueryAnchor? {
        await withCheckedContinuation { continuation in
            // Create an anchored query with nil anchor and limit=0
            // This returns the anchor without fetching sample data
            let query = HKAnchoredObjectQuery(
                type: type,
                predicate: nil,
                anchor: nil,
                limit: 0  // We don't need data, just the anchor
            ) { _, _, _, newAnchor, error in
                if let error = error {
                    print("⚠️ Error capturing anchor for \(type.identifier): \(error)")
                    continuation.resume(returning: nil)
                    return
                }
                continuation.resume(returning: newAnchor)
            }
            healthStore.execute(query)
        }
    }
}

// MARK: - HealthCheckable

extension HealthKitManager: HealthCheckable {
    var healthCheckName: String {
        "HealthKitManager"
    }

    func performHealthCheck() -> HealthStatus {
        // Check if stream is enabled
        guard configProvider.isStreamEnabled("healthkit") else {
            return .disabled
        }

        // Check authorization
        guard isAuthorized else {
            return .unhealthy(reason: "HealthKit not authorized")
        }

        // Check if monitoring should be running
        let shouldBeMonitoring = true
        let actuallyMonitoring = healthTimer != nil

        if shouldBeMonitoring && !actuallyMonitoring {
            // Attempt recovery
            startMonitoring()
            return .unhealthy(reason: "Monitoring stopped unexpectedly, restarting")
        }

        return .healthy
    }
}