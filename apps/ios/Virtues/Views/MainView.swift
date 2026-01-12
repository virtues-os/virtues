//
//  MainView.swift
//  Virtues
//
//  Main dashboard view after onboarding
//

import SwiftUI

struct MainView: View {
    @ObservedObject private var deviceManager = DeviceManager.shared
    @ObservedObject private var healthKitManager = HealthKitManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @ObservedObject private var lowPowerModeMonitor = LowPowerModeMonitor.shared
    @ObservedObject private var permissionMonitor = PermissionMonitor.shared

    @State private var showingSettings = false
    @State private var isManualSyncing = false
    
    var body: some View {
        NavigationView {
            ScrollView {
                VStack(spacing: 20) {
                    // Status Card
                    StatusCard()
                        .padding(.horizontal)

                    // Low Power Mode Warning
                    if lowPowerModeMonitor.isLowPowerModeEnabled {
                        LowPowerModeWarningBanner()
                            .padding(.horizontal)
                    }

                    // Permission Issues Warning
                    if permissionMonitor.hasIssues {
                        ForEach(permissionMonitor.currentIssues) { issue in
                            PermissionIssuesBanner(issue: issue)
                                .padding(.horizontal)
                        }
                    }

                    // Quick Stats
                    QuickStatsView()
                        .padding(.horizontal)
                    
                    // Manual Sync Button
                    ManualSyncButton(
                        isManualSyncing: $isManualSyncing,
                        onSync: performManualSync
                    )
                    .padding(.horizontal)
                    
                    // Data Collection Status
                    DataCollectionStatus()
                        .padding(.horizontal)
                    
                    // Debug Info (if enabled)
                    #if DEBUG
                    DebugInfoSection()
                        .padding(.horizontal)
                    #endif
                }
                .padding(.vertical)
            }
            .navigationTitle("Virtues")
            .navigationBarTitleDisplayMode(.large)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: { showingSettings = true }) {
                        Image(systemName: "gearshape")
                    }
                }
            }
            .sheet(isPresented: $showingSettings) {
                SettingsView()
            }
        }
        .navigationViewStyle(StackNavigationViewStyle())
    }
    
    private func performManualSync() {
        Task {
            await MainActor.run {
                isManualSyncing = true
            }
            
            print("🔄 Manual sync triggered")
            
            // Trigger full sync and upload (collects from all streams)
            await uploadCoordinator.triggerManualUpload()
            
            await MainActor.run {
                isManualSyncing = false
            }
        }
    }
}

// MARK: - Status Card

/// Timer container with guaranteed cleanup via deinit
class CountdownTimerContainer: ObservableObject {
    @Published var secondsUntilSync: Int = 0
    var timer: ReliableTimer?

    deinit {
        timer?.cancel()
        timer = nil
    }

    func invalidate() {
        timer?.cancel()
        timer = nil
    }
}

struct StatusCard: View {
    @ObservedObject private var deviceManager = DeviceManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @StateObject private var timerContainer = CountdownTimerContainer()
    
    var statusColor: Color {
        if !deviceManager.isConfigured {
            return .warmError
        } else if uploadCoordinator.isUploading {
            return .warmInfo
        } else {
            return .warmSuccess
        }
    }
    
    var statusText: String {
        if !deviceManager.isConfigured {
            return "Not Connected"
        } else if uploadCoordinator.isUploading {
            return "Syncing..."
        } else {
            return "Connected"
        }
    }
    
    var countdownText: String {
        let minutes = timerContainer.secondsUntilSync / 60
        let seconds = timerContainer.secondsUntilSync % 60
        return String(format: "Next sync: %d:%02d", minutes, seconds)
    }
    
    var body: some View {
        VStack(spacing: 16) {
            HStack {
                Circle()
                    .fill(statusColor)
                    .frame(width: 12, height: 12)
                
                Text(statusText)
                    .font(.headline)
                
                Spacer()
                
                if deviceManager.isConfigured && !uploadCoordinator.isUploading {
                    Text(countdownText)
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
            }
            
            if deviceManager.isConfigured {
                HStack {
                    Label(deviceManager.configuration.deviceName, systemImage: "iphone")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)

                    Spacer()

                    Text(deviceManager.configuration.apiEndpoint)
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
        .onAppear {
            startCountdownTimer()
        }
        .onChange(of: uploadCoordinator.lastUploadDate) {
            updateCountdown()
        }
    }

    private func startCountdownTimer() {
        updateCountdown()
        timerContainer.timer = ReliableTimer.builder()
            .interval(1.0)
            .qos(.utility)
            .handler { [weak timerContainer, weak uploadCoordinator] in
                guard let timerContainer = timerContainer,
                      let uploadCoordinator = uploadCoordinator else { return }

                // Update countdown - uses captured values safely
                let syncInterval: TimeInterval = 300

                DispatchQueue.main.async {
                    if let lastUpload = uploadCoordinator.lastUploadDate {
                        let nextSyncTime = lastUpload.addingTimeInterval(syncInterval)
                        let remainingTime = nextSyncTime.timeIntervalSince(Date())

                        if remainingTime > 0 {
                            timerContainer.secondsUntilSync = Int(remainingTime)
                        } else {
                            timerContainer.secondsUntilSync = 0
                        }
                    } else {
                        timerContainer.secondsUntilSync = Int(syncInterval)
                    }
                }
            }
            .build()
    }

    private func updateCountdown() {
        let syncInterval: TimeInterval = 300 // 5 minutes

        if let lastUpload = uploadCoordinator.lastUploadDate {
            let nextSyncTime = lastUpload.addingTimeInterval(syncInterval)
            let remainingTime = nextSyncTime.timeIntervalSince(Date())

            if remainingTime > 0 {
                timerContainer.secondsUntilSync = Int(remainingTime)
            } else {
                timerContainer.secondsUntilSync = 0
            }
        } else {
            // If no last upload, assume sync will happen soon
            timerContainer.secondsUntilSync = Int(syncInterval)
        }
    }
}

// MARK: - Quick Stats

struct QuickStatsView: View {
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared

    var body: some View {
        HStack(spacing: 16) {
            StatCard(
                title: "HealthKit",
                value: "\(uploadCoordinator.streamCounts.healthkit)",
                icon: "heart.fill",
                color: .warmError
            )

            StatCard(
                title: "Location",
                value: "\(uploadCoordinator.streamCounts.location)",
                icon: "location.fill",
                color: .warmInfo
            )

            StatCard(
                title: "Audio",
                value: "\(uploadCoordinator.streamCounts.audio)",
                icon: "mic.fill",
                color: .warmSuccess
            )
        }
    }
}

struct StatCard: View {
    let title: String
    let value: String
    let icon: String
    let color: Color
    
    var body: some View {
        VStack(spacing: 4) {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.caption)
                    .foregroundColor(color)
                
                Text(value)
                    .font(.headline)
                    .bold()
            }
            
            Text(title)
                .font(.caption2)
                .foregroundColor(.warmForegroundMuted)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 8)
        .padding(.horizontal, 12)
        .background(color.opacity(0.15))
        .cornerRadius(12)
    }
}

// MARK: - Manual Sync Button

struct ManualSyncButton: View {
    @Binding var isManualSyncing: Bool
    let onSync: () -> Void
    
    var body: some View {
        Button(action: onSync) {
            HStack {
                if isManualSyncing {
                    ProgressView()
                        .progressViewStyle(CircularProgressViewStyle())
                        .scaleEffect(0.8)
                } else {
                    Image(systemName: "arrow.triangle.2.circlepath")
                }
                
                Text("Sync Now")
                    .font(.headline)
            }
            .frame(maxWidth: .infinity)
            .padding()
            .background(Color.warmPrimary)
            .foregroundColor(.white)
            .cornerRadius(12)
        }
        .disabled(isManualSyncing)
    }
}

// MARK: - Data Collection Status

struct DataCollectionStatus: View {
    @ObservedObject private var healthKitManager = HealthKitManager.shared
    @ObservedObject private var locationManager = LocationManager.shared
    @ObservedObject private var audioManager = AudioManager.shared
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Data Collection")
                .h2Style()
            
            VStack(spacing: 8) {
                DataStreamRow(
                    name: "HealthKit",
                    isActive: healthKitManager.isAuthorized,
                    lastSync: healthKitManager.lastSyncDate
                )
                
                DataStreamRow(
                    name: "Location",
                    isActive: locationManager.isTracking,
                    lastSync: locationManager.lastSaveDate
                )
                
                DataStreamRow(
                    name: "Audio",
                    isActive: audioManager.isRecording,
                    lastSync: audioManager.lastSaveDate
                )
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }
}

struct DataStreamRow: View {
    let name: String
    let isActive: Bool
    let lastSync: Date?

    var body: some View {
        HStack {
            Label(name, systemImage: isActive ? "checkmark.circle.fill" : "xmark.circle")
                .foregroundColor(isActive ? .warmSuccess : .warmError)

            Spacer()

            if let lastSync = lastSync {
                Text(lastSync, style: .relative)
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
            }
        }
    }
}

// MARK: - Low Power Mode Warning

struct LowPowerModeWarningBanner: View {
    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "bolt.slash.fill")
                .font(.title2)
                .foregroundColor(.warmWarning)

            VStack(alignment: .leading, spacing: 4) {
                Text("Low Power Mode Active")
                    .font(.headline)
                    .foregroundColor(.warmForeground)

                Text("Uploads paused to save battery. Disable Low Power Mode to resume syncing.")
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()
        }
        .padding()
        .background(Color.warmWarningSubtle)
        .cornerRadius(12)
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color.warmWarning.opacity(0.3), lineWidth: 1)
        )
    }
}

// MARK: - Debug Info

struct DebugInfoSection: View {
    @ObservedObject private var deviceManager = DeviceManager.shared
    @ObservedObject private var networkMonitor = NetworkMonitor.shared
    @State private var isExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Button(action: { isExpanded.toggle() }) {
                HStack {
                    Label("Debug Info", systemImage: "hammer")
                        .h2Style()

                    Spacer()

                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .foregroundColor(.warmForegroundMuted)
                }
            }

            if isExpanded {
                VStack(alignment: .leading, spacing: 8) {
                    // Network info
                    Text("Network Quality: \(networkMonitor.networkQualityDescription)")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundColor(.warmForegroundMuted)

                    Text("Batch Size: \(networkMonitor.currentBatchSize) events")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundColor(.warmForegroundMuted)

                    Divider()

                    // Device info
                    Text(deviceManager.getDebugInfo())
                        .font(.system(.caption, design: .monospaced))
                        .foregroundColor(.warmForegroundMuted)
                }
                .padding()
                .background(Color.warmBorderSubtle)
                .cornerRadius(8)
            }
        }
        .padding()
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }
}

// MARK: - Preview

struct MainView_Previews: PreviewProvider {
    static var previews: some View {
        MainView()
    }
}
