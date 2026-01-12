//
//  OnboardingView.swift
//  Virtues
//
//  Onboarding flow with 3 steps: endpoint config, permissions, initial sync
//

import SwiftUI

struct OnboardingView: View {
    @ObservedObject private var deviceManager = DeviceManager.shared
    @ObservedObject private var healthKitManager = HealthKitManager.shared

    @State private var currentStep = 1
    @State private var apiEndpoint = ""
    @State private var isVerifying = false
    @State private var errorMessage: String?
    @State private var syncProgress: Double = 0
    @State private var hasRequestedPermissions = false

    @Binding var isOnboardingComplete: Bool
    
    var body: some View {
        NavigationView {
            VStack(alignment: .leading, spacing: 0) {
                // Custom title
                Text("Setup Virtues")
                    .font(.system(size: 34, weight: .bold, design: .serif))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 20)
                    .padding(.top, 60)
                    .padding(.bottom, 20)

                // Progress indicator
                ProgressIndicator(currentStep: currentStep, totalSteps: 3)
                    .padding(.horizontal)
                    .padding(.top)

                // Content based on current step
                Group {
                    switch currentStep {
                    case 1:
                        ServerConfigurationStep(
                            apiEndpoint: $apiEndpoint,
                            isVerifying: $isVerifying,
                            errorMessage: $errorMessage,
                            deviceId: deviceManager.deviceId,
                            onNext: verifyConfiguration
                        )
                    case 2:
                        PermissionsStep(
                            hasRequestedPermissions: $hasRequestedPermissions,
                            onNext: moveToInitialSync
                        )
                    case 3:
                        InitialSyncStep(
                            syncProgress: $syncProgress,
                            onComplete: completeOnboarding
                        )
                    default:
                        EmptyView()
                    }
                }
                .transition(.asymmetric(
                    insertion: .move(edge: .trailing),
                    removal: .move(edge: .leading)
                ))
                
                Spacer()
            }
            .navigationBarHidden(true)
        }
        .navigationViewStyle(StackNavigationViewStyle())
    }
    
    // MARK: - Actions

    private func verifyConfiguration() {
        Task {
            await MainActor.run {
                isVerifying = true
                errorMessage = nil

                // Update device configuration with the server URL
                deviceManager.updateConfiguration(apiEndpoint: apiEndpoint)
            }

            // Verify the connection
            let success = await deviceManager.verifyConfiguration()

            await MainActor.run {
                isVerifying = false

                if success {
                    // Connection successful, move to permissions
                    withAnimation {
                        currentStep = 2
                    }
                } else {
                    errorMessage = deviceManager.lastError
                }
            }
        }
    }
    
    private func moveToInitialSync() {
        withAnimation {
            currentStep = 3
        }
        
        // Start initial sync
        performInitialSync()
    }
    
    private func performInitialSync() {
        Task {
            // Upload batches as they're saved
            let uploadTask = Task {
                // Give initial sync a moment to start saving batches
                try? await Task.sleep(nanoseconds: 2_000_000_000) // 2 seconds
                
                // Keep uploading while initial sync is running
                while healthKitManager.isSyncing {
                    print("🚀 Uploading batches during initial sync")
                    await BatchUploadCoordinator.shared.performUpload()
                    try? await Task.sleep(nanoseconds: 5_000_000_000) // 5 seconds between uploads
                }
            }
            
            let success = await healthKitManager.performInitialSync { progress in
                Task { @MainActor in
                    withAnimation {
                        self.syncProgress = progress
                    }
                }
            }
            
            // Cancel upload task if still running
            uploadTask.cancel()
            
            if success {
                print("✅ Initial sync completed successfully")
                
                // Final upload to ensure everything is sent
                print("🚀 Final upload after initial sync")
                await BatchUploadCoordinator.shared.performUpload()
                
                // Now start regular monitoring (AFTER initial sync is done)
                print("🏥 Starting regular HealthKit monitoring")
                healthKitManager.startMonitoring()
                
                // Start the upload coordinator for background syncing
                BatchUploadCoordinator.shared.startPeriodicUploads()
                
                // Small delay for UI
                try? await Task.sleep(nanoseconds: 500_000_000)
                
                await MainActor.run {
                    completeOnboarding()
                }
            } else {
                print("❌ Initial sync failed")
            }
        }
    }
    
    private func completeOnboarding() {
        withAnimation {
            isOnboardingComplete = true
        }
    }
}

// MARK: - Progress Indicator

struct ProgressIndicator: View {
    let currentStep: Int
    let totalSteps: Int

    var body: some View {
        HStack(spacing: 8) {
            ForEach(1...totalSteps, id: \.self) { step in
                Circle()
                    .fill(step <= currentStep ? Color.warmPrimary : Color.warmBorder)
                    .frame(width: 10, height: 10)

                if step < totalSteps {
                    Rectangle()
                        .fill(step < currentStep ? Color.warmPrimary : Color.warmBorder)
                        .frame(height: 2)
                }
            }
        }
        .padding(.vertical)
    }
}

// MARK: - Step 1: Server Configuration

struct ServerConfigurationStep: View {
    @Binding var apiEndpoint: String
    @Binding var isVerifying: Bool
    @Binding var errorMessage: String?
    let deviceId: String
    let onNext: () -> Void

    @State private var showCopiedToast = false

    var isValid: Bool {
        !apiEndpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            Text("Connect to Server")
                .h2Style()
                .padding(.horizontal)

            VStack(alignment: .leading, spacing: 16) {
                // Server URL
                VStack(alignment: .leading, spacing: 8) {
                    Label("Server URL", systemImage: "network")
                        .font(.headline)

                    TextField("https://your-server.com", text: $apiEndpoint)
                        .font(.system(size: 17))
                        .padding()
                        .frame(minHeight: 52)
                        .background(Color.warmSurfaceElevated)
                        .cornerRadius(10)
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(Color.warmBorder, lineWidth: 0.5)
                        )
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                        .keyboardType(.URL)

                    Text("The URL of your Virtues server")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }

                // Device ID (read-only, copyable)
                VStack(alignment: .leading, spacing: 8) {
                    Label("Device ID", systemImage: "iphone")
                        .font(.headline)

                    HStack {
                        Text(deviceId)
                            .font(.system(size: 14, weight: .medium, design: .monospaced))
                            .foregroundColor(.warmForegroundMuted)
                            .lineLimit(1)
                            .truncationMode(.middle)

                        Spacer()

                        Button(action: {
                            Haptics.light()
                            UIPasteboard.general.string = deviceId
                            showCopiedToast = true
                            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                                showCopiedToast = false
                            }
                        }) {
                            HStack(spacing: 4) {
                                Image(systemName: "doc.on.doc")
                                Text("Copy")
                                    .font(.caption)
                            }
                            .foregroundColor(.warmPrimary)
                        }
                        .buttonStyle(PlainButtonStyle())
                    }
                    .padding()
                    .frame(minHeight: 52)
                    .background(Color.warmSurfaceElevated)
                    .cornerRadius(10)
                    .overlay(
                        RoundedRectangle(cornerRadius: 10)
                            .stroke(Color.warmBorder, lineWidth: 0.5)
                    )

                    Text("Copy this ID and enter it in the web app to link this device")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
            }
            .padding(.horizontal)

            // Error message
            if let error = errorMessage {
                HStack {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundColor(.warmError)
                    Text(error)
                        .font(.caption)
                        .foregroundColor(.warmError)
                }
                .padding(.horizontal)
                .padding(.vertical, 8)
                .background(Color.warmErrorSubtle)
                .cornerRadius(8)
                .padding(.horizontal)
            }

            // Connect button
            Button(action: onNext) {
                HStack {
                    if isVerifying {
                        ProgressView()
                            .progressViewStyle(CircularProgressViewStyle())
                            .scaleEffect(0.8)
                    } else {
                        Text("Connect")
                        Image(systemName: "arrow.right")
                    }
                }
                .frame(maxWidth: .infinity)
                .padding()
                .background(isValid ? Color.warmPrimary : Color.warmBorder)
                .foregroundColor(.white)
                .cornerRadius(12)
            }
            .disabled(!isValid || isVerifying)
            .padding(.horizontal)

            Spacer()
        }
        .padding(.top)
        .overlay(alignment: .bottom) {
            if showCopiedToast {
                Text("Device ID copied to clipboard")
                    .font(.subheadline)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .background(Color.warmSurface)
                    .cornerRadius(8)
                    .shadow(radius: 4)
                    .padding(.bottom, 20)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .animation(.easeInOut(duration: 0.2), value: showCopiedToast)
            }
        }
    }
}

// MARK: - Step 2: Permissions

struct PermissionsStep: View {
    @ObservedObject private var healthKitManager = HealthKitManager.shared
    @ObservedObject private var locationManager = LocationManager.shared
    @ObservedObject private var audioManager = AudioManager.shared
    @Binding var hasRequestedPermissions: Bool
    let onNext: () -> Void
    
    @State private var showingSettings = false
    @Environment(\.scenePhase) var scenePhase
    
    var allPermissionsGranted: Bool {
        let healthGranted = healthKitManager.hasAllPermissions()
        let locationGranted = locationManager.hasAlwaysPermission  // Require "Always" permission, not just "When In Use"
        let audioGranted = audioManager.hasPermission

        #if DEBUG
        print("📋 Permission check: health=\(healthGranted), location=\(locationGranted) (hasPermission=\(locationManager.hasPermission), hasAlways=\(locationManager.hasAlwaysPermission)), audio=\(audioGranted)")
        #endif

        return healthGranted && locationGranted && audioGranted
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            Text("Grant Permissions")
                .h2Style()
                .padding(.horizontal)
            
            Text("Virtues needs the following permissions to track your data:")
                .font(.body)
                .foregroundColor(.warmForegroundMuted)
                .padding(.horizontal)
            
            VStack(spacing: 16) {
                // HealthKit Permission
                PermissionRow(
                    icon: "heart.fill",
                    title: "Health Data",
                    description: "Heart rate, steps, sleep, energy, and more",
                    isGranted: healthKitManager.hasAllPermissions()
                )
                
                // Location Permission
                PermissionRow(
                    icon: "location.fill",
                    title: "Location (Always)",
                    description: "Track your movements and locations",
                    isGranted: locationManager.hasAlwaysPermission  // Check for "Always", not just "When In Use"
                )
                
                // Microphone Permission
                PermissionRow(
                    icon: "mic.fill",
                    title: "Microphone",
                    description: "Record and transcribe audio",
                    isGranted: audioManager.hasPermission
                )
            }
            .padding(.horizontal)
            
            // Request permissions button or Continue button
            if !hasRequestedPermissions {
                Button(action: requestAllPermissions) {
                    HStack {
                        Text("Continue")
                        Image(systemName: "arrow.right")
                    }
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(Color.warmPrimary)
                    .foregroundColor(.white)
                    .cornerRadius(12)
                }
                .padding(.horizontal)
            } else {
                // Continue button (always available after permissions requested)
                VStack(spacing: 12) {
                    // Show info message if some permissions were denied
                    if !allPermissionsGranted {
                        if locationManager.hasPermission && !locationManager.hasAlwaysPermission {
                            Text("Some features require \"Always\" location for background tracking")
                                .font(.caption)
                                .foregroundColor(.warmForegroundMuted)
                                .multilineTextAlignment(.center)
                        } else {
                            Text("Some features may be limited without all permissions")
                                .font(.caption)
                                .foregroundColor(.warmForegroundMuted)
                                .multilineTextAlignment(.center)
                        }
                    }

                    Button(action: onNext) {
                        HStack {
                            Text("Continue")
                            Image(systemName: "arrow.right")
                        }
                        .frame(maxWidth: .infinity)
                        .padding()
                        .background(allPermissionsGranted ? Color.warmSuccess : Color.warmPrimary)
                        .foregroundColor(.white)
                        .cornerRadius(12)
                    }
                }
                .padding(.horizontal)
            }
            
            Spacer()
        }
        .padding(.top)
        .onChange(of: scenePhase) { oldValue, newValue in
            if newValue == .active && hasRequestedPermissions {
                // Re-check permissions when returning from Settings
                healthKitManager.checkAuthorizationStatus()
                locationManager.checkAuthorizationStatus()
                audioManager.checkAuthorizationStatus()
            }
        }
    }
    
    private func requestAllPermissions() {
        Task {
            // Request all permissions in sequence

            // 1. Request HealthKit
            print("📱 Requesting HealthKit permission...")
            _ = await healthKitManager.requestAuthorization()

            // Small delay to let iOS permission system update
            try? await Task.sleep(nanoseconds: 500_000_000) // 0.5 seconds

            // Force re-check of HealthKit status
            healthKitManager.checkAuthorizationStatus()

            // 2. Request Location - Two-step process (Apple best practice)
            print("📱 Requesting Location permission (When In Use)...")
            _ = await locationManager.requestAuthorization()  // Step 1: When In Use

            // Small delay to let iOS permission system update
            try? await Task.sleep(nanoseconds: 500_000_000) // 0.5 seconds

            // Force re-check of Location status
            locationManager.checkAuthorizationStatus()

            // Step 2: Upgrade to Always (if When In Use was granted)
            if locationManager.authorizationStatus == .authorizedWhenInUse {
                print("📱 Upgrading Location permission to Always...")
                _ = await locationManager.requestAlwaysAuthorization()  // Step 2: Always

                // Small delay to let iOS permission system update
                try? await Task.sleep(nanoseconds: 500_000_000) // 0.5 seconds

                // Force re-check of Location status
                locationManager.checkAuthorizationStatus()
            }

            // 3. Request Microphone
            print("📱 Requesting Microphone permission...")
            _ = await audioManager.requestAuthorization()

            // Small delay to let iOS permission system update
            try? await Task.sleep(nanoseconds: 500_000_000) // 0.5 seconds

            // Force re-check of Audio status
            audioManager.checkAuthorizationStatus()

            await MainActor.run {
                hasRequestedPermissions = true

                // Log final permission states for debugging
                print("📱 Final permission states:")
                print("   HealthKit: \(healthKitManager.hasAllPermissions())")
                print("   Location: \(locationManager.hasPermission)")
                print("   Microphone: \(audioManager.hasPermission)")
            }
        }
    }
    
}

struct PermissionRow: View {
    let icon: String
    let title: String
    let description: String
    let isGranted: Bool

    var body: some View {
        HStack(spacing: 16) {
            Image(systemName: icon)
                .font(.title2)
                .foregroundColor(isGranted ? .warmSuccess : .warmWarning)
                .frame(width: 30)

            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.headline)
                Text(description)
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
            }

            Spacer()

            Image(systemName: isGranted ? "checkmark.circle.fill" : "xmark.circle")
                .foregroundColor(isGranted ? .warmSuccess : .warmError)
                .font(.title2)
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }
}

// MARK: - Step 3: Initial Sync

struct InitialSyncStep: View {
    @Binding var syncProgress: Double
    let onComplete: () -> Void
    @ObservedObject private var deviceManager = DeviceManager.shared
    
    var progressPercentage: Int {
        Int(syncProgress * 100)
    }
    
    var isComplete: Bool {
        syncProgress >= 1.0
    }
    
    var syncDays: Int {
        deviceManager.configuration.getInitialSyncDays(for: "healthkit")
    }
    
    var body: some View {
        VStack(spacing: 32) {
            Text("Syncing Health Data")
                .h2Style()
            
            Text(isComplete ? "Sync complete! Ready to start tracking." : "Fetching the last \(syncDays) days of health data...")
                .font(.body)
                .foregroundColor(.warmForegroundMuted)
                .multilineTextAlignment(.center)

            // Progress circle
            ZStack {
                Circle()
                    .stroke(Color.warmBorder, lineWidth: 20)
                    .frame(width: 200, height: 200)

                Circle()
                    .trim(from: 0, to: syncProgress)
                    .stroke(isComplete ? Color.warmSuccess : Color.warmPrimary, style: StrokeStyle(lineWidth: 20, lineCap: .round))
                    .frame(width: 200, height: 200)
                    .rotationEffect(.degrees(-90))
                    .animation(.easeInOut(duration: 0.5), value: syncProgress)

                VStack {
                    if isComplete {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.system(size: 60))
                            .foregroundColor(.warmSuccess)
                    } else {
                        Text("\(progressPercentage)%")
                            .font(.system(size: 48, weight: .bold, design: .rounded))
                        Text("Complete")
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)
                    }
                }
            }

            if !isComplete {
                VStack(spacing: 8) {
                    Image(systemName: "info.circle")
                        .foregroundColor(.warmInfo)
                    Text("Keep the app open during initial sync")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                    Text("This ensures all data is uploaded successfully")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
                .padding()
                .background(Color.warmInfoSubtle)
                .cornerRadius(12)
                .padding(.horizontal)
            }

            if isComplete {
                Button(action: onComplete) {
                    HStack {
                        Text("Get Started")
                        Image(systemName: "arrow.right")
                    }
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(Color.warmSuccess)
                    .foregroundColor(.white)
                    .cornerRadius(12)
                }
                .padding(.horizontal)
            }
            
            Spacer()
        }
        .padding(.top, 48)
        .onChange(of: syncProgress) { oldValue, newValue in
            // Auto-complete when reaching 100%
            if newValue >= 1.0 && oldValue < 1.0 {
                // Give a small delay to show the completion state
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
                    onComplete()
                }
            }
        }
    }
}

// MARK: - Preview

struct OnboardingView_Previews: PreviewProvider {
    static var previews: some View {
        OnboardingView(isOnboardingComplete: .constant(false))
    }
}

