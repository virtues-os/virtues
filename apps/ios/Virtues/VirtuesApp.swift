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
    @StateObject private var deviceManager = DeviceManager.shared
    @Environment(\.scenePhase) private var scenePhase
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
        _ = FinanceKitManager.shared
        _ = LocationManager.shared
        _ = AudioManager.shared
        _ = BatteryManager.shared
        _ = ContactsManager.shared
        _ = EventKitManager.shared
        _ = PermissionMonitor.shared
        _ = LowPowerModeMonitor.shared
        HealthCheckCoordinator.shared.startMonitoring()
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

                // Check if this app version meets the server minimum
                Task {
                    await deviceManager.checkMinimumVersion()
                }
            }
            .fullScreenCover(isPresented: Binding(
                get: { deviceManager.updateRequired },
                set: { _ in }
            )) {
                UpdateRequiredView()
            }
            .onChange(of: scenePhase) { _, newPhase in
                if newPhase == .active {
                    // Re-check version when user returns from TestFlight update
                    Task {
                        await deviceManager.checkMinimumVersion()
                    }
                }
            }
        }
    }
    
    private func registerBackgroundTasks() {
        // Register background refresh task
        BGTaskScheduler.shared.register(
            forTaskWithIdentifier: "com.virtues.ios.refresh",
            using: nil
        ) { task in
            guard let refreshTask = task as? BGAppRefreshTask else {
                print("❌ Unexpected task type for refresh task")
                task.setTaskCompleted(success: false)
                return
            }
            handleBackgroundRefresh(task: refreshTask)
        }

        // Register background processing task
        BGTaskScheduler.shared.register(
            forTaskWithIdentifier: "com.virtues.ios.processing",
            using: nil
        ) { task in
            guard let processingTask = task as? BGProcessingTask else {
                print("❌ Unexpected task type for processing task")
                task.setTaskCompleted(success: false)
                return
            }
            handleBackgroundProcessing(task: processingTask)
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
        let financeKitManager = FinanceKitManager.shared
        let batteryManager = BatteryManager.shared
        let contactsManager = ContactsManager.shared
        let eventKitManager = EventKitManager.shared

        // Schedule background refresh task (required for background execution)
        scheduleBackgroundRefresh()

        // Start periodic uploads (this now handles all data collection)
        uploadCoordinator.startPeriodicUploads()

        // Start location tracking if authorized
        if locationManager.hasPermission {
            locationManager.startTracking()
        }

        // Start audio recording if authorized
        if audioManager.hasPermission {
            audioManager.startRecording()
        }

        // Start HealthKit monitoring if authorized
        if healthKitManager.isAuthorized {
            healthKitManager.startMonitoring()
        }

        // Start FinanceKit monitoring if authorized
        if financeKitManager.isAuthorized {
            financeKitManager.startMonitoring()
        }

        // Start battery monitoring (no permission required)
        batteryManager.startMonitoring()

        // Start barometer monitoring if available (no permission required)
        if BarometerManager.isAvailable {
            BarometerManager.shared.startMonitoring()
        }

        // Start EventKit monitoring if authorized
        if eventKitManager.hasAnyPermission {
            eventKitManager.startMonitoring()
        }

        // Sync contacts if authorized
        if contactsManager.isAuthorized {
            Task {
                await contactsManager.syncIfNeeded()
            }
        }
    }
}

// MARK: - Update Required View

struct UpdateRequiredView: View {
    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            Image(systemName: "arrow.down.app")
                .font(.system(size: 56))
                .foregroundColor(.orange)

            Text("Update Required")
                .font(.system(.title, design: .serif))
                .fontWeight(.semibold)

            Text("A newer version of Virtues is required to continue. Please update from TestFlight.")
                .font(.body)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)

            Button(action: {
                // Open TestFlight
                if let url = URL(string: "itms-beta://") {
                    UIApplication.shared.open(url)
                }
            }) {
                Text("Open TestFlight")
                    .fontWeight(.semibold)
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(Color.orange)
                    .foregroundColor(.white)
                    .cornerRadius(12)
            }
            .padding(.horizontal, 40)

            Spacer()
        }
        .background(Color.warmBackground)
    }
}
