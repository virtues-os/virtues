//
//  VirtuesApp.swift
//  Virtues
//
//  Created by Adam Jace on 7/30/25.
//

import SwiftUI
import BackgroundTasks
import UIKit

@main
struct VirtuesApp: App {
    @StateObject private var lifecycleObserver = AppLifecycleObserver()
    @AppStorage("hasSeenWelcome") private var hasSeenWelcome = false

    init() {
        // Configure navigation bar appearance with serif fonts
        configureNavigationBarAppearance()

        // Register background tasks on app launch
        registerBackgroundTasks()

        // Initialize all managers to ensure singletons are created
        _ = DeviceManager.shared
        _ = NetworkMonitor.shared  // Initialize network monitoring early
        _ = BatchUploadCoordinator.shared
        _ = HealthKitManager.shared
        _ = LocationManager.shared
        _ = AudioManager.shared
        _ = BatteryManager.shared
        _ = ContactsManager.shared
        _ = PermissionMonitor.shared
        _ = LowPowerModeMonitor.shared
    }

    private func configureNavigationBarAppearance() {
        // iOS 26: Use default Liquid Glass appearance, only customize fonts
        // Don't set background - let the system handle the glass effect
        let appearance = UINavigationBarAppearance()
        appearance.configureWithTransparentBackground()  // Let Liquid Glass show through

        // Warm foreground color for navigation text
        let warmForeground = UIColor(red: 0x26/255.0, green: 0x25/255.0, blue: 0x1E/255.0, alpha: 1.0)

        // Serif font for inline title (compact mode)
        appearance.titleTextAttributes = [
            .font: UIFont(descriptor: UIFontDescriptor.preferredFontDescriptor(withTextStyle: .headline).withDesign(.serif)!, size: 0),
            .foregroundColor: warmForeground
        ]

        // Serif font for large title
        appearance.largeTitleTextAttributes = [
            .font: UIFont(descriptor: UIFontDescriptor.preferredFontDescriptor(withTextStyle: .largeTitle).withDesign(.serif)!, size: 0),
            .foregroundColor: warmForeground
        ]

        UINavigationBar.appearance().standardAppearance = appearance
        UINavigationBar.appearance().scrollEdgeAppearance = appearance
        UINavigationBar.appearance().compactAppearance = appearance

        // Set tint color for navigation bar items (back button, etc.)
        UINavigationBar.appearance().tintColor = UIColor(red: 0xEB/255.0, green: 0x56/255.0, blue: 0x01/255.0, alpha: 1.0)

        // Prefer inline (compact) titles by default
        UINavigationBar.appearance().prefersLargeTitles = false
    }

    var body: some Scene {
        WindowGroup {
            Group {
                if hasSeenWelcome {
                    ContentView()
                } else {
                    WelcomeView(onComplete: {
                        // Auto-enable permission-free sensors
                        BatteryManager.shared.startMonitoring()
                        if BarometerManager.isAvailable {
                            BarometerManager.shared.startMonitoring()
                        }
                        hasSeenWelcome = true
                    })
                }
            }
            .preferredColorScheme(.light)  // Force light mode for warm color palette
            .background(Color.warmBackground)
            .onAppear {
                // Start services based on current configuration
                startAllServices()
            }
        }
    }
    
    private func registerBackgroundTasks() {
        // Register background refresh task
        BGTaskScheduler.shared.register(
            forTaskWithIdentifier: "com.virtues.ios.refresh",
            using: nil
        ) { task in
            handleBackgroundRefresh(task: task as! BGAppRefreshTask)
        }
        
        // Register background processing task
        BGTaskScheduler.shared.register(
            forTaskWithIdentifier: "com.virtues.ios.processing",
            using: nil
        ) { task in
            handleBackgroundProcessing(task: task as! BGProcessingTask)
        }
    }
    
    private func handleBackgroundRefresh(task: BGAppRefreshTask) {
        // Schedule next refresh
        scheduleBackgroundRefresh()

        // Perform quick sync
        let syncTask = Task {
            await BatchUploadCoordinator.shared.performUpload()
        }

        task.expirationHandler = {
            syncTask.cancel()
        }

        Task {
            _ = await syncTask.result
            task.setTaskCompleted(success: true)
        }
    }

    private func handleBackgroundProcessing(task: BGProcessingTask) {
        // Perform longer running tasks
        let processingTask = Task {
            await BatchUploadCoordinator.shared.performUpload()
        }

        task.expirationHandler = {
            processingTask.cancel()
        }

        Task {
            _ = await processingTask.result
            task.setTaskCompleted(success: true)
        }
    }
    
    private func scheduleBackgroundRefresh() {
        let request = BGAppRefreshTaskRequest(identifier: "com.virtues.ios.refresh")
        request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60) // 15 minutes
        
        do {
            try BGTaskScheduler.shared.submit(request)
        } catch {
            #if DEBUG
            print("Failed to schedule background refresh: \(error)")
            #endif
        }
    }
    
    private func startAllServices() {
        let deviceManager = DeviceManager.shared
        let uploadCoordinator = BatchUploadCoordinator.shared
        let locationManager = LocationManager.shared
        let audioManager = AudioManager.shared
        let healthKitManager = HealthKitManager.shared
        let batteryManager = BatteryManager.shared
        let contactsManager = ContactsManager.shared

        // Schedule background refresh task (required for background execution)
        scheduleBackgroundRefresh()

        // Start periodic uploads (this now handles all data collection)
        uploadCoordinator.startPeriodicUploads()

        let config = deviceManager.configuration

        #if DEBUG
        print("🚀 Starting services with configuration:")
        print("   Stream configurations: \(config.streamConfigurations.count) streams")
        for (key, streamConfig) in config.streamConfigurations {
            print("     - \(key): enabled=\(streamConfig.enabled), initialSyncDays=\(streamConfig.initialSyncDays)")
        }
        #endif

        // Start location tracking if authorized AND enabled
        if locationManager.hasPermission && config.isStreamEnabled("location") {
            locationManager.startTracking()
        }

        // Start audio recording if authorized AND enabled
        if audioManager.hasPermission && config.isStreamEnabled("microphone") {
            audioManager.startRecording()
        }

        // Start HealthKit monitoring if authorized AND enabled
        if healthKitManager.isAuthorized && config.isStreamEnabled("healthkit") {
            healthKitManager.startMonitoring()
        }

        // Start battery monitoring if enabled (no permission required)
        if config.isStreamEnabled("battery") {
            batteryManager.startMonitoring()
        }

        // Sync contacts if authorized AND enabled
        if contactsManager.isAuthorized && config.isStreamEnabled("contacts") {
            Task {
                await contactsManager.syncIfNeeded()
            }
        }
    }
}
