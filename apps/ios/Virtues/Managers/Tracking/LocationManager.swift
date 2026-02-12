//
//  LocationManager.swift
//  Virtues
//
//  Handles location tracking and permissions
//

import Foundation
import CoreLocation
import Combine
import UIKit

class LocationManager: NSObject, ObservableObject {
    static let shared = LocationManager()

    // MARK: - Constants
    private let samplingIntervalSeconds = 15.0  // Battery optimization: ~10-15% savings vs 10s
    private let healthCheckIntervalSeconds = 30.0
    private let locationFreshnessThresholdSeconds = 60.0

    @Published var authorizationStatus: CLAuthorizationStatus = .notDetermined
    @Published var isTracking = false
    @Published var lastSaveDate: Date?

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader

    private let locationManager = CLLocationManager()
    private var locationContinuation: AsyncStream<CLLocation>.Continuation?
    private var locationTimer: ReliableTimer?
    private var lastLocation: CLLocation?
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    private let timerQueue = DispatchQueue(label: "com.virtues.location.timer", qos: .userInitiated)

    // Authorization continuation for proper async/await handling
    private var authorizationContinuation: CheckedContinuation<Void, Never>?
    private let authorizationTimeoutSeconds: UInt64 = 30  // 30 second timeout for user interaction

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         dataUploader: DataUploader) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.dataUploader = dataUploader

        super.init()

        locationManager.delegate = self
        // Match the requirements: kCLLocationAccuracyNearestTenMeters
        locationManager.desiredAccuracy = kCLLocationAccuracyNearestTenMeters
        locationManager.distanceFilter = kCLDistanceFilterNone
        locationManager.allowsBackgroundLocationUpdates = true
        locationManager.pausesLocationUpdatesAutomatically = false
        locationManager.showsBackgroundLocationIndicator = true
        locationManager.activityType = .other

        // Check initial status
        authorizationStatus = locationManager.authorizationStatus

        // Register with centralized health check coordinator
        HealthCheckCoordinator.shared.register(self)
    }

    /// Legacy singleton initializer - uses default dependencies
    private override convenience init() {
        self.init(
            configProvider: DeviceManager.shared,
            storageProvider: SQLiteManager.shared,
            dataUploader: BatchUploadCoordinator.shared
        )
    }

    deinit {
        locationTimer?.cancel()
        locationTimer = nil
        HealthCheckCoordinator.shared.unregister(self)
    }
    
    // MARK: - Authorization

    func requestAuthorization() async -> Bool {
        // If already authorized (either When In Use or Always), return immediately
        if authorizationStatus == .authorizedAlways || authorizationStatus == .authorizedWhenInUse {
            return true
        }

        // If already denied or restricted, don't show dialog again
        if authorizationStatus == .denied || authorizationStatus == .restricted {
            return false
        }

        // Wait for the delegate callback (with timeout as failsafe)
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            self.authorizationContinuation = continuation

            // STEP 1: Request "When In Use" first (Apple best practice)
            Task { @MainActor in
                self.locationManager.requestWhenInUseAuthorization()
            }

            // Timeout failsafe - in case user dismisses dialog without responding
            Task {
                try? await Task.sleep(nanoseconds: self.authorizationTimeoutSeconds * 1_000_000_000)
                // Only resume if we haven't already (delegate didn't fire)
                if self.authorizationContinuation != nil {
                    self.authorizationContinuation?.resume()
                    self.authorizationContinuation = nil
                }
            }
        }

        // Accept When In Use as success for initial request
        return authorizationStatus == .authorizedAlways || authorizationStatus == .authorizedWhenInUse
    }

    /// Request upgrade from "When In Use" to "Always" permission
    func requestAlwaysAuthorization() async -> Bool {
        // If already Always, return immediately
        if authorizationStatus == .authorizedAlways {
            return true
        }

        // Must have "When In Use" before requesting "Always"
        guard authorizationStatus == .authorizedWhenInUse else {
            return false
        }

        // Wait for the delegate callback (with timeout as failsafe)
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            self.authorizationContinuation = continuation

            // STEP 2: Request upgrade to "Always"
            Task { @MainActor in
                self.locationManager.requestAlwaysAuthorization()
            }

            // Timeout failsafe
            Task {
                try? await Task.sleep(nanoseconds: self.authorizationTimeoutSeconds * 1_000_000_000)
                if self.authorizationContinuation != nil {
                    self.authorizationContinuation?.resume()
                    self.authorizationContinuation = nil
                }
            }
        }

        return authorizationStatus == .authorizedAlways
    }

    var hasPermission: Bool {
        // Accept either When In Use or Always
        // We request Always, but iOS may grant When In Use first
        return authorizationStatus == .authorizedAlways ||
               authorizationStatus == .authorizedWhenInUse
    }

    var hasAlwaysPermission: Bool {
        // Check specifically for Always authorization (required for background tracking)
        return authorizationStatus == .authorizedAlways
    }
    
    // MARK: - Location Tracking
    
    func startTracking() {
        guard hasPermission else {
            print("❌ Location tracking requires authorization")
            return
        }

        print("📍 Starting location tracking")
        isTracking = true
        locationManager.startUpdatingLocation()
        
        // Start the timer for location sampling
        print("⏱️ Starting location timer with \(samplingIntervalSeconds)-second interval")
        startLocationTimer()
    }
    
    func stopTracking() {
        print("📍 Stopping location tracking")
        isTracking = false
        locationManager.stopUpdatingLocation()

        // Stop the timer
        stopLocationTimer()
    }
}

// MARK: - CLLocationManagerDelegate

extension LocationManager: CLLocationManagerDelegate {
    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        // Delegate is always called on main thread, so update synchronously
        // This fixes race condition where authorizationStatus wasn't updated before checks
        authorizationStatus = manager.authorizationStatus

        // Resume any pending authorization continuation
        if let continuation = authorizationContinuation {
            authorizationContinuation = nil
            continuation.resume()
        }
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        // Store the most recent location for sampling
        if let location = locations.last {
            lastLocation = location
            locationContinuation?.yield(location)
        }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        print("❌ Location manager error: \(error.localizedDescription)")
    }

    func locationManagerDidPauseLocationUpdates(_ manager: CLLocationManager) {
        print("⚠️ iOS paused location updates - this is expected in low-power scenarios")
        // Don't restart immediately - iOS will resume when appropriate
        // Health check will handle recovery if needed
    }

    func locationManagerDidResumeLocationUpdates(_ manager: CLLocationManager) {
        print("✅ iOS resumed location updates")
        // Ensure timer is running
        if isTracking && locationTimer == nil {
            print("🔄 Restarting location timer after iOS resume")
            startLocationTimer()
        }
    }
}

// MARK: - Location Sampling

extension LocationManager {
    private func startLocationTimer() {
        // Cancel any existing timer
        locationTimer?.cancel()

        print("⏱️ Creating location timer that fires every \(Int(samplingIntervalSeconds)) seconds")
        locationTimer = ReliableTimer.builder()
            .interval(samplingIntervalSeconds)
            .queue(timerQueue)
            .handler { [weak self] in
                print("⏰ Location timer fired - sampling location...")
                self?.sampleCurrentLocation()
            }
            .build()

        print("✅ Location timer started on dedicated queue")
    }

    private func stopLocationTimer() {
        locationTimer?.cancel()
        locationTimer = nil
        print("⏹️ Location timer stopped")
    }

    private func sampleCurrentLocation() {
        // Piggyback upload flush on sampling timer (~15s) — time-gated to 5 min inside uploadIfNeeded
        dataUploader.uploadIfNeeded()

        guard let location = lastLocation else {
            print("⚠️ No location available to sample")
            return
        }

        // Only sample if the location is fresh (within threshold)
        let age = Date().timeIntervalSince(location.timestamp)
        guard age < locationFreshnessThresholdSeconds else {
            print("⚠️ Location too old to sample (age: \(age) seconds)")
            return
        }

        // Validate coordinates are within valid bounds
        guard isValidCoordinate(location) else {
            print("⚠️ Invalid coordinates: lat=\(location.coordinate.latitude), lon=\(location.coordinate.longitude)")
            return
        }

        // Create location data
        let locationData = LocationData(location: location)

        print("📍 Sampled location: lat=\(location.coordinate.latitude), lon=\(location.coordinate.longitude), speed=\(location.speed), course=\(location.course)")

        // Save directly to SQLite
        saveLocationSample(locationData)
    }

    /// Validate that coordinates are within valid geographic bounds
    private func isValidCoordinate(_ location: CLLocation) -> Bool {
        let lat = location.coordinate.latitude
        let lon = location.coordinate.longitude
        // Valid latitude: -90 to 90, valid longitude: -180 to 180
        return lat >= -90 && lat <= 90 && lon >= -180 && lon <= 180
    }
    
    private func saveLocationSample(_ locationData: LocationData) {
        // Begin background task to ensure save completes
        beginBackgroundTask()

        let deviceId = configProvider.deviceId

        // Attempt to save with async retry mechanism
        Task {
            let result = await saveWithRetry(location: locationData, deviceId: deviceId, maxAttempts: 3)

            switch result {
            case .success:
                print("✅ Saved location sample to SQLite queue")
                await MainActor.run {
                    self.lastSaveDate = Date()
                }
                dataUploader.updateUploadStats()

            case .failure(let error):
                ErrorLogger.shared.log(error, deviceId: deviceId)
            }

            endBackgroundTask()
        }
    }

    /// Attempts to save location sample with exponential backoff retry (async, non-blocking)
    private func saveWithRetry(location: LocationData, deviceId: String, maxAttempts: Int) async -> Result<Void, AnyDataCollectionError> {
        let streamData = CoreLocationStreamData(
            deviceId: deviceId,
            locations: [location]
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
                    streamType: .location,
                    underlyingError: error,
                    dataSize: nil
                )
                return .failure(AnyDataCollectionError(encodingError))
            }

            // Attempt to save to SQLite
            let success = storageProvider.enqueue(streamName: "ios_location", data: data)

            if success {
                if attempt > 1 {
                    ErrorLogger.shared.logSuccessfulRetry(streamType: .location, attemptNumber: attempt)
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
            streamType: .location,
            reason: "Failed to enqueue to SQLite after \(maxAttempts) attempts",
            attemptNumber: maxAttempts
        )
        return .failure(AnyDataCollectionError(storageError))
    }
    
    private func beginBackgroundTask() {
        backgroundTask = UIApplication.shared.beginBackgroundTask { [weak self] in
            self?.endBackgroundTask()
        }
    }

    private func endBackgroundTask() {
        if backgroundTask != .invalid {
            UIApplication.shared.endBackgroundTask(backgroundTask)
            backgroundTask = .invalid
        }
    }
}

// MARK: - HealthCheckable

extension LocationManager: HealthCheckable {
    var healthCheckName: String {
        "LocationManager"
    }

    func performHealthCheck() -> HealthStatus {
        // Check permission
        guard hasPermission else {
            return .disabled
        }

        if !isTracking {
            // Attempt recovery
            stopTracking()
            startTracking()
            return .unhealthy(reason: "Tracking stopped unexpectedly, restarting")
        } else {
            // Verify we're getting fresh location updates
            if let lastLoc = lastLocation {
                let age = Date().timeIntervalSince(lastLoc.timestamp)
                if age > 60 {
                    stopTracking()
                    startTracking()
                    return .unhealthy(reason: "Location data stale (\(Int(age))s old), restarting")
                }
            }

            // Verify timer is still running
            if locationTimer == nil {
                startLocationTimer()
                return .unhealthy(reason: "Timer stopped unexpectedly, restarting")
            }

            return .healthy
        }
    }
}