//
//  AudioManager.swift
//  Virtues
//
//  Handles audio recording and microphone permissions
//

import Foundation
import AVFoundation
import AVFAudio
import UIKit

class AudioManager: NSObject, ObservableObject {
    static let shared = AudioManager()

    // MARK: - Constants
    private let chunkDurationSeconds = 300.0  // 5 minutes per chunk
    private let healthCheckIntervalSeconds = 30.0
    private let interruptionRecoveryDelay = 0.5

    // MARK: - Published Properties
    @Published var microphoneAuthorizationStatus: AVAudioApplication.recordPermission = .undetermined
    @Published var isRecording = false
    @Published var lastSaveDate: Date?
    @Published var currentDbLevel: Float = -160  // For real-time VU meter (dB scale: -160 to 0)

    // MARK: - dB Metering
    private var dbMeteringTimer: ReliableTimer?
    private var accumulatedDbSamples: [Float] = []  // For historical chart

    // MARK: - Dependencies
    private let configProvider: ConfigurationProvider
    private let storageProvider: StorageProvider
    private let dataUploader: DataUploader

    private let audioSession = AVAudioSession.sharedInstance()
    private var audioRecorder: AVAudioRecorder?
    private var recordingTimer: ReliableTimer?
    private var currentChunkStartTime: Date?
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    private let timerQueue = DispatchQueue(label: "com.virtues.audio.timer", qos: .userInitiated)
    private var pausedForOtherAudio = false

    /// Initialize with dependency injection
    init(configProvider: ConfigurationProvider,
         storageProvider: StorageProvider,
         dataUploader: DataUploader) {
        self.configProvider = configProvider
        self.storageProvider = storageProvider
        self.dataUploader = dataUploader

        super.init()

        checkAuthorizationStatus()
        setupNotificationObservers()

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
        NotificationCenter.default.removeObserver(self)
        HealthCheckCoordinator.shared.unregister(self)
    }
    
    // MARK: - Authorization
    
    func requestAuthorization() async -> Bool {
        let granted = await AVAudioApplication.requestRecordPermission()
        // Use await MainActor.run to ensure status is updated before returning
        // This fixes race condition where hasPermission was checked before status was updated
        await MainActor.run {
            self.checkAuthorizationStatus()
        }
        return granted
    }
    
    func checkAuthorizationStatus() {
        microphoneAuthorizationStatus = AVAudioApplication.shared.recordPermission
    }

    var hasPermission: Bool {
        return microphoneAuthorizationStatus == .granted
    }
    
    // MARK: - Notification Observers

    private func setupNotificationObservers() {
        // Listen for audio interruptions (phone calls, Siri, etc.)
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleInterruption),
            name: AVAudioSession.interruptionNotification,
            object: audioSession
        )

        // Listen for other apps playing audio (Spotify, Apple Music, etc.)
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleSilenceSecondaryAudioHint),
            name: AVAudioSession.silenceSecondaryAudioHintNotification,
            object: audioSession
        )

        // Listen for app becoming active (returning from background)
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleAppBecameActive),
            name: UIApplication.didBecomeActiveNotification,
            object: nil
        )
    }

    @objc private func handleAppBecameActive(notification: Notification) {
        #if DEBUG
        print("📱 App became active - checking if recording should resume")
        #endif

        // Check if we should be recording but aren't
        let shouldBeRecording = configProvider.isStreamEnabled("microphone") && hasPermission

        if shouldBeRecording && !isRecording {
            #if DEBUG
            print("   Recording was interrupted, attempting to resume...")
            #endif

            // Small delay to let audio session settle
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.startRecording()
            }
        }
    }

    @objc private func handleInterruption(notification: Notification) {
        guard let info = notification.userInfo,
              let typeValue = info[AVAudioSessionInterruptionTypeKey] as? UInt,
              let type = AVAudioSession.InterruptionType(rawValue: typeValue) else {
            return
        }

        #if DEBUG
        print("🎙️ Audio interruption: \(type == .began ? "BEGAN" : "ENDED")")
        #endif

        switch type {
        case .began:
            // Interruption started (phone call, Siri, etc.)
            // Recording will be automatically stopped by the system
            #if DEBUG
            print("🎙️ Audio interruption began - recording paused by system")
            #endif

        case .ended:
            // Interruption ended - check if we should resume
            guard let optionsValue = info[AVAudioSessionInterruptionOptionKey] as? UInt else {
                // No options provided - trigger health check as fallback
                DispatchQueue.main.asyncAfter(deadline: .now() + interruptionRecoveryDelay) { [weak self] in
                    _ = self?.performHealthCheck()
                }
                return
            }

            let options = AVAudioSession.InterruptionOptions(rawValue: optionsValue)

            if options.contains(.shouldResume) {
                // System says we should resume - do it immediately
                #if DEBUG
                print("🎙️ Audio interruption ended - resuming recording immediately")
                #endif

                // Resume recording with minimal delay
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
                    guard let self = self else { return }

                    // Reactivate audio session
                    do {
                        try self.audioSession.setActive(true)

                        // Check if we should be recording (stream enabled and has permission)
                        let shouldBeRecording = self.configProvider.isStreamEnabled("microphone") && self.hasPermission

                        // If we should be recording but aren't, restart
                        if shouldBeRecording && !self.isRecording {
                            self.startRecording()
                        }
                    } catch {
                        #if DEBUG
                        print("❌ Failed to reactivate audio session: \(error)")
                        #endif

                        // Fallback to health check
                        _ = self.performHealthCheck()
                    }
                }
            } else {
                // System says don't resume - wait for health check
                #if DEBUG
                print("🎙️ Audio interruption ended - waiting for health check")
                #endif

                DispatchQueue.main.asyncAfter(deadline: .now() + interruptionRecoveryDelay) { [weak self] in
                    _ = self?.performHealthCheck()
                }
            }

        @unknown default:
            break
        }
    }

    @objc private func handleSilenceSecondaryAudioHint(notification: Notification) {
        guard let userInfo = notification.userInfo,
              let typeValue = userInfo[AVAudioSessionSilenceSecondaryAudioHintTypeKey] as? UInt,
              let type = AVAudioSession.SilenceSecondaryAudioHintType(rawValue: typeValue) else {
            return
        }

        switch type {
        case .begin:
            // Other audio (Spotify, Apple Music) started playing
            #if DEBUG
            print("Other audio started - pausing recording to preserve audio quality")
            #endif

            if isRecording {
                pausedForOtherAudio = true
                stopRecording()
            }

        case .end:
            // Other audio stopped playing
            #if DEBUG
            print("Other audio stopped - resuming recording")
            #endif

            // Only auto-resume if we paused for other audio and stream is enabled
            if pausedForOtherAudio && configProvider.isStreamEnabled("microphone") && hasPermission {
                pausedForOtherAudio = false
                startRecording()
            }

        @unknown default:
            break
        }
    }

    // MARK: - Audio Session Setup

    func setupAudioSession() throws {
        // Simple setup - iPhone built-in mic only
        // No Bluetooth options to avoid AirPods interference when connected to other devices
        try audioSession.setCategory(.playAndRecord, mode: .default)
        try audioSession.setActive(true)
    }
    
    // MARK: - Recording Control
    
    func startRecording() {
        guard hasPermission else {
            print("❌ Microphone permission not granted")
            return
        }

        // Don't start if already recording
        guard !isRecording else {
            return
        }

        #if DEBUG
        print("🎙️ startRecording() called: hasPermission=\(hasPermission), isRecording=\(isRecording)")
        #endif

        do {
            try setupAudioSession()
            startRecordingChunk()
            isRecording = true
            #if DEBUG
            print("✅ Audio recording started successfully")
            #endif
        } catch {
            print("❌ Failed to start recording: \(error)")
            isRecording = false
        }
    }
    
    func stopRecording() {
        recordingTimer?.cancel()
        recordingTimer = nil
        stopDbMetering()

        // Save any partial chunk before stopping
        savePartialChunkIfNeeded()

        audioRecorder = nil
        currentChunkStartTime = nil
        isRecording = false
    }

    // MARK: - dB Metering

    private func startDbMetering() {
        dbMeteringTimer = ReliableTimer.builder()
            .interval(0.5)  // Poll every 500ms
            .queue(timerQueue)
            .handler { [weak self] in
                self?.updateDbLevel()
            }
            .build()
    }

    private func stopDbMetering() {
        dbMeteringTimer?.cancel()
        dbMeteringTimer = nil
    }

    private func updateDbLevel() {
        guard let recorder = audioRecorder, recorder.isRecording else { return }

        recorder.updateMeters()
        let db = recorder.averagePower(forChannel: 0)

        DispatchQueue.main.async { [weak self] in
            guard let self = self else { return }
            self.currentDbLevel = db
            self.accumulatedDbSamples.append(db)
        }
    }

}

// MARK: - Chunk Recording

extension AudioManager {
    private func savePartialChunkIfNeeded() {
        guard let recorder = audioRecorder,
              let startTime = currentChunkStartTime else {
            return
        }

        // Only stop if actually recording
        if recorder.isRecording {
            recorder.stop()
        }

        // Try to save the partial chunk
        if let audioData = try? Data(contentsOf: recorder.url) {
            let chunk = AudioChunk(
                startDate: startTime,
                endDate: Date(),
                audioData: audioData,
                overlapDuration: 0.0
            )
            saveAudioChunk(chunk)
            try? FileManager.default.removeItem(at: recorder.url)
        }
    }

    private func startRecordingChunk() {
        // Create audio file URL
        let documentsPath = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let audioFilename = documentsPath.appendingPathComponent("chunk_\(Date().timeIntervalSince1970).m4a")

        // Configure recording settings for 16kHz sample rate with optimized compression
        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
            AVSampleRateKey: 16000.0,
            AVNumberOfChannelsKey: 1,
            AVEncoderAudioQualityKey: AVAudioQuality.low.rawValue, // Low quality is fine for speech
            AVEncoderBitRateKey: 16000 // 16kbps - optimal for speech transcription
        ]

        do {
            audioRecorder = try AVAudioRecorder(url: audioFilename, settings: settings)
            audioRecorder?.delegate = self
            audioRecorder?.isMeteringEnabled = true  // Enable dB metering
            audioRecorder?.record()

            currentChunkStartTime = Date()
            accumulatedDbSamples.removeAll()

            // Start dB metering timer (polls every 500ms for VU meter updates)
            startDbMetering()

            // Use ReliableTimer for background execution
            recordingTimer = ReliableTimer.builder()
                .interval(chunkDurationSeconds)
                .queue(timerQueue)
                .oneTime(true)  // One-time timer for chunk duration
                .handler { [weak self] in
                    self?.finishCurrentChunk()
                }
                .build()
        } catch {
            print("❌ Failed to start recording chunk: \(error)")
        }
    }
    
    private func finishCurrentChunk() {
        guard let recorder = audioRecorder,
              let startTime = currentChunkStartTime else {
            print("⚠️ No active recorder or start time when finishing chunk")
            return
        }

        // Check recording state
        let wasRecording = recorder.isRecording
        recorder.stop()
        let endTime = Date()
        let recorderUrl = recorder.url

        // Calculate average dB for this chunk from accumulated samples
        let avgDb = accumulatedDbSamples.isEmpty ? currentDbLevel : accumulatedDbSamples.reduce(0, +) / Float(accumulatedDbSamples.count)
        accumulatedDbSamples.removeAll()

        let duration = endTime.timeIntervalSince(startTime)
        #if DEBUG
        print("📊 Finishing chunk: wasRecording=\(wasRecording), duration=\(duration)s, avgDb=\(avgDb)")
        #endif

        // Process the recorded audio
        do {
            let audioData = try Data(contentsOf: recorderUrl)

            // Validate audio data - minimum ~1KB for valid audio
            guard audioData.count > 1000 else {
                print("❌ Audio file too small (\(audioData.count) bytes), discarding chunk")
                try? FileManager.default.removeItem(at: recorderUrl)
                if isRecording { startRecordingChunk() }
                return
            }

            #if DEBUG
            print("💾 Saving audio chunk: \(audioData.count) bytes, duration=\(duration)s")
            #endif

            let chunkStartTime = startTime
            let chunkEndTime = endTime

            Task { @MainActor [weak self] in
                guard let self = self else { return }

                // Create chunk object with 2-second overlap for transcription continuity
                let chunk = AudioChunk(
                    startDate: chunkStartTime,
                    endDate: chunkEndTime,
                    audioData: audioData,
                    overlapDuration: 2.0,
                    averageDbLevel: avgDb
                )

                // Save directly to SQLite
                self.saveAudioChunk(chunk)

                // Clean up temporary file
                try? FileManager.default.removeItem(at: recorderUrl)
            }
        } catch {
            print("❌ Failed to read audio file: \(error)")
            try? FileManager.default.removeItem(at: recorderUrl)
        }

        // Continue recording if still active
        if isRecording {
            startRecordingChunk()
        }
    }
    
    private func saveAudioChunk(_ chunk: AudioChunk) {
        // Begin background task
        beginBackgroundTask()

        let deviceId = configProvider.deviceId

        // Attempt to save with retry mechanism (async)
        Task {
            let result = await saveWithRetry(chunk: chunk, deviceId: deviceId, maxAttempts: 3)

            switch result {
            case .success:
                await MainActor.run {
                    self.lastSaveDate = Date()
                }
                dataUploader.updateUploadStats()

            case .failure(let error):
                ErrorLogger.shared.log(error, deviceId: deviceId)
            }

            // End background task when done
            endBackgroundTask()
        }
    }

    /// Attempts to save audio chunk with exponential backoff retry
    private func saveWithRetry(chunk: AudioChunk, deviceId: String, maxAttempts: Int) async -> Result<Void, AnyDataCollectionError> {
        let streamData = AudioStreamData(
            deviceId: deviceId,
            chunks: [chunk]
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
                    streamType: .audio,
                    underlyingError: error,
                    dataSize: chunk.audioData.count
                )
                return .failure(AnyDataCollectionError(encodingError))
            }

            // Attempt to save to SQLite
            let success = storageProvider.enqueue(streamName: "ios_mic", data: data)

            if success {
                if attempt > 1 {
                    ErrorLogger.shared.logSuccessfulRetry(streamType: .audio, attemptNumber: attempt)
                }
                return .success
            }

            // If not last attempt, wait before retrying (non-blocking async sleep)
            if attempt < maxAttempts {
                let delay = Double(attempt) * 0.5  // 0.5s, 1.0s backoff
                try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            }
        }

        // All attempts failed
        let storageError = StorageError(
            streamType: .audio,
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

// MARK: - AVAudioRecorderDelegate

extension AudioManager: AVAudioRecorderDelegate {
    func audioRecorderDidFinishRecording(_ recorder: AVAudioRecorder, successfully flag: Bool) {
        if !flag {
            print("❌ Audio recording finished with error")
        }
    }

    func audioRecorderEncodeErrorDidOccur(_ recorder: AVAudioRecorder, error: Error?) {
        if let error = error {
            print("❌ Audio encoding error: \(error)")
        }
    }
}

// MARK: - HealthCheckable

extension AudioManager: HealthCheckable {
    var healthCheckName: String {
        "AudioManager"
    }

    func performHealthCheck() -> HealthStatus {
        // Check if stream is enabled
        guard configProvider.isStreamEnabled("microphone") else {
            return .disabled
        }

        // Check permission
        guard hasPermission else {
            return .unhealthy(reason: "Microphone permission not granted")
        }

        // Check recording state
        let shouldBeRecording = configProvider.isStreamEnabled("microphone") && hasPermission
        let actuallyRecording = audioRecorder?.isRecording ?? false

        if shouldBeRecording && !actuallyRecording {
            // Attempt recovery
            stopRecording()
            startRecording()
            return .unhealthy(reason: "Recording stopped unexpectedly, restarting")
        } else if !shouldBeRecording && actuallyRecording {
            stopRecording()
            return .healthy
        }

        return .healthy
    }
}
