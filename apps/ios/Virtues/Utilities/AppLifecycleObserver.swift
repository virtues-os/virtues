//
//  AppLifecycleObserver.swift
//  Virtues
//
//  Observes app lifecycle events with proper cleanup
//

import Foundation
import UIKit

/// Observes app lifecycle events with guaranteed cleanup via deinit
class AppLifecycleObserver: ObservableObject {
    private var observers: [NSObjectProtocol] = []
    private var backgroundUploadTask: UIBackgroundTaskIdentifier = .invalid

    init() {
        setupObservers()
    }

    private func setupObservers() {
        // Observe foreground transitions
        let foregroundObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.willEnterForegroundNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.handleWillEnterForeground()
        }
        observers.append(foregroundObserver)

        // Observe background transitions — flush queued data before iOS suspends us
        let backgroundObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didEnterBackgroundNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.handleDidEnterBackground()
        }
        observers.append(backgroundObserver)
    }

    private func handleWillEnterForeground() {
        // Restart periodic uploads (idempotent — stops first, clears error flags, fires immediate upload)
        BatchUploadCoordinator.shared.startPeriodicUploads()

        // Trigger health check for all services when returning to foreground
        // Delay slightly to ensure system is ready after returning to foreground
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            // Audio health check will handle any needed recovery
            _ = AudioManager.shared.performHealthCheck()
            // Location health check ensures tracking continues (especially important after network changes)
            _ = LocationManager.shared.performHealthCheck()
        }
    }

    private func handleDidEnterBackground() {
        // End any existing background task to avoid orphaning it on rapid cycling
        endBackgroundUploadTask()

        // Request ~30s execution window to flush queued data before suspension
        let taskId = UIApplication.shared.beginBackgroundTask { [weak self] in
            self?.endBackgroundUploadTask()
        }
        backgroundUploadTask = taskId

        Task {
            await BatchUploadCoordinator.shared.performUpload()
            await MainActor.run { [weak self] in
                guard let self = self else { return }
                // Only end if this is still the same task (not replaced by a newer one)
                if self.backgroundUploadTask == taskId {
                    self.endBackgroundUploadTask()
                }
            }
        }
    }

    private func endBackgroundUploadTask() {
        if backgroundUploadTask != .invalid {
            UIApplication.shared.endBackgroundTask(backgroundUploadTask)
            backgroundUploadTask = .invalid
        }
    }

    deinit {
        // Guaranteed cleanup of all observers
        for observer in observers {
            NotificationCenter.default.removeObserver(observer)
        }
        observers.removeAll()
    }
}
