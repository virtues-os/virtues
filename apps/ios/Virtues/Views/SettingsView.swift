//
//  SettingsView.swift
//  Virtues
//
//  Settings and configuration view
//

import SwiftUI

struct SettingsView: View {
    @ObservedObject private var deviceManager = DeviceManager.shared
    @ObservedObject private var healthKitManager = HealthKitManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @ObservedObject private var locationManager = LocationManager.shared
    @ObservedObject private var audioManager = AudioManager.shared
    @ObservedObject private var contactsManager = ContactsManager.shared
    @ObservedObject private var financeKitManager = FinanceKitManager.shared
    @ObservedObject private var eventKitManager = EventKitManager.shared

    @State private var showingResetAlert = false
    @State private var showingStorageDetails = false
    @State private var showingEndpointEdit = false
    @State private var showQRScanner = false
    @State private var isCompletingPairing = false
    @State private var pairingError: String?
    @State private var showCopiedToast = false
    
    var body: some View {
        NavigationView {
            Form {
                // Server Section
                Section(header: Text("Server")) {
                    // Connection Status
                    HStack {
                        Text("Status")
                        Spacer()
                        if deviceManager.isConfigured {
                            HStack(spacing: 4) {
                                Image(systemName: "checkmark.circle.fill")
                                    .foregroundColor(.warmSuccess)
                                Text("Connected")
                                    .foregroundColor(.warmSuccess)
                            }
                        } else {
                            HStack(spacing: 4) {
                                Image(systemName: "xmark.circle.fill")
                                    .foregroundColor(.warmError)
                                Text("Not Connected")
                                    .foregroundColor(.warmError)
                            }
                        }
                    }

                    // Server URL
                    HStack {
                        Text("Server URL")
                        Spacer()
                        if deviceManager.isConfigured {
                            Text(deviceManager.configuration.apiEndpoint)
                                .font(.caption)
                                .foregroundColor(.warmForegroundMuted)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        } else {
                            Text("Not set")
                                .foregroundColor(.warmForegroundMuted)
                        }
                    }

                    // QR Scan to pair (primary action)
                    Button(action: {
                        Haptics.light()
                        pairingError = nil
                        showQRScanner = true
                    }) {
                        HStack {
                            if isCompletingPairing {
                                ProgressView()
                                    .progressViewStyle(CircularProgressViewStyle())
                                    .scaleEffect(0.8)
                                Text("Connecting...")
                            } else {
                                Label("Scan QR Code to Pair", systemImage: "qrcode.viewfinder")
                            }
                        }
                        .foregroundColor(.warmPrimary)
                    }
                    .disabled(isCompletingPairing)

                    if let error = pairingError {
                        HStack {
                            Image(systemName: "exclamationmark.triangle")
                                .foregroundColor(.warmError)
                            Text(error)
                                .font(.caption)
                                .foregroundColor(.warmError)
                        }
                    }

                    // Manual endpoint edit (secondary)
                    Button(action: {
                        Haptics.light()
                        showingEndpointEdit = true
                    }) {
                        Label(
                            deviceManager.isConfigured ? "Edit Server Manually" : "Manual Setup",
                            systemImage: "link"
                        )
                        .foregroundColor(.warmForegroundMuted)
                        .font(.subheadline)
                    }
                }
                
                // Permissions Section
                Section(header: Text("Permissions")) {
                    PermissionStatusRow(
                        title: "HealthKit",
                        status: healthKitManager.isAuthorized ? .granted : .denied
                    )
                    
                    PermissionStatusRow(
                        title: "Location (Always)",
                        status: locationManager.hasAlwaysPermission
                            ? .granted
                            : (locationManager.hasPermission ? .partial : .denied)
                    )
                    
                    PermissionStatusRow(
                        title: "Microphone",
                        status: audioManager.hasPermission ? .granted : .denied
                    )

                    PermissionStatusRow(
                        title: "Contacts",
                        status: contactsManager.isAuthorized ? .granted : .denied
                    )

                    PermissionStatusRow(
                        title: "FinanceKit",
                        status: financeKitManager.isAuthorized ? .granted : .denied
                    )

                    PermissionStatusRow(
                        title: "EventKit",
                        status: eventKitManager.hasAnyPermission ? .granted : .denied
                    )

                    Button(action: {
                        Haptics.light()
                        openAppSettings()
                    }) {
                        Label("Open iOS Settings", systemImage: "gear")
                    }
                }
                
                // Storage Section
                Section(header: Text("Storage")) {
                    HStack {
                        Text("Pending")
                        Spacer()
                        Text("\(uploadCoordinator.uploadStats.pending) records (\(uploadCoordinator.getQueueSizeString()))")
                            .foregroundColor(.warmForegroundMuted)
                    }

                    Button(action: {
                        Haptics.light()
                        showingStorageDetails = true
                    }) {
                        Label("Storage Details", systemImage: "info.circle")
                    }
                }
                
                // Sync Settings
                Section(header: Text("Sync Settings")) {
                    HStack {
                        Text("Auto Sync")
                        Spacer()
                        Text("Every 5 minutes")
                            .foregroundColor(.warmForegroundMuted)
                    }

                    if let lastUpload = uploadCoordinator.lastUploadDate {
                        HStack {
                            Text("Last Upload")
                            Spacer()
                            Text(lastUpload, style: .relative)
                                .foregroundColor(.warmForegroundMuted)
                        }
                    }
                }

                // About Section
                Section(header: Text("About")) {
                    HStack {
                        Text("Version")
                        Spacer()
                        Text(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0")
                            .foregroundColor(.warmForegroundMuted)
                    }

                    HStack {
                        Text("Build")
                        Spacer()
                        Text(Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "1")
                            .foregroundColor(.warmForegroundMuted)
                    }
                }
                
                // Actions Section
                Section {
                    Button(action: {
                        Haptics.warning()
                        showingResetAlert = true
                    }) {
                        Label("Reset App", systemImage: "exclamationmark.triangle")
                            .foregroundColor(.warmError)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Color.warmBackground)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .alert("Reset App?", isPresented: $showingResetAlert) {
                Button("Cancel", role: .cancel) { }
                Button("Reset", role: .destructive) {
                    resetApp()
                }
            } message: {
                Text("This will clear all settings and require you to set up the app again. Pending uploads will be lost.")
            }
            .sheet(isPresented: $showingStorageDetails) {
                StorageDetailsView()
            }
            .sheet(isPresented: $showingEndpointEdit) {
                EndpointEditView()
            }
            .fullScreenCover(isPresented: $showQRScanner) {
                QRScannerView(
                    onScanned: handleQRScanResult,
                    onCancel: { showQRScanner = false }
                )
            }
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
        .navigationViewStyle(StackNavigationViewStyle())
    }
    
    private func handleQRScanResult(endpoint: String, sourceId: String) {
        showQRScanner = false
        isCompletingPairing = true
        pairingError = nil

        Task {
            do {
                let _ = try await NetworkManager.shared.completePairing(
                    endpoint: endpoint,
                    sourceId: sourceId,
                    deviceId: DeviceManager.shared.deviceId
                )

                await MainActor.run {
                    deviceManager.updateConfiguration(apiEndpoint: endpoint)
                    deviceManager.isConfigured = true
                    deviceManager.configurationState = .configured
                    isCompletingPairing = false
                    Haptics.success()
                }
            } catch {
                await MainActor.run {
                    isCompletingPairing = false
                    if let networkError = error as? NetworkError {
                        pairingError = networkError.errorDescription
                    } else {
                        pairingError = error.localizedDescription
                    }
                    Haptics.error()
                }
            }
        }
    }

    private func openAppSettings() {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
    }
    
    private func resetApp() {
        // Stop all services (this stops all data collection)
        uploadCoordinator.stopPeriodicUploads()

        // Stop individual trackers
        locationManager.stopTracking()
        audioManager.stopRecording()

        // Clear configuration (disconnects from server)
        deviceManager.clearConfiguration()

        // Clear UserDefaults
        if let bundleId = Bundle.main.bundleIdentifier {
            UserDefaults.standard.removePersistentDomain(forName: bundleId)
        }
    }
}

// MARK: - Permission Status Row

enum PermissionDisplayStatus {
    case granted
    case partial
    case denied

    var label: String {
        switch self {
        case .granted:
            return "Granted"
        case .partial:
            return "Limited"
        case .denied:
            return "Denied"
        }
    }

    var color: Color {
        switch self {
        case .granted:
            return .warmSuccess
        case .partial:
            return .warmWarning
        case .denied:
            return .warmError
        }
    }

    var iconName: String {
        switch self {
        case .granted:
            return "checkmark.circle.fill"
        case .partial:
            return "exclamationmark.circle.fill"
        case .denied:
            return "xmark.circle"
        }
    }
}

struct PermissionStatusRow: View {
    let title: String
    let status: PermissionDisplayStatus

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            HStack(spacing: 4) {
                Image(systemName: status.iconName)
                    .foregroundColor(status.color)
                Text(status.label)
                    .font(.caption)
                    .foregroundColor(status.color)
            }
        }
    }
}

// MARK: - Storage Details View

struct StorageDetailsView: View {
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @Environment(\.dismiss) var dismiss

    private let sqliteManager = SQLiteManager.shared

    @State private var databaseSize: String = "Calculating..."
    @State private var availableStorage: String = "Calculating..."

    var body: some View {
        NavigationView {
            List {
                Section(header: Text("Upload Queue")) {
                    DetailRow(
                        label: "Pending",
                        value: "\(uploadCoordinator.uploadStats.pending) events"
                    )
                    
                    DetailRow(
                        label: "Failed",
                        value: "\(uploadCoordinator.uploadStats.failed) events"
                    )
                    
                    DetailRow(
                        label: "Total Size",
                        value: uploadCoordinator.getQueueSizeString()
                    )
                }
                
                Section(header: Text("Storage")) {
                    DetailRow(
                        label: "Database Size",
                        value: databaseSize
                    )
                    
                    DetailRow(
                        label: "Available Storage",
                        value: availableStorage
                    )
                }
                
                Section(header: Text("Cleanup Policy")) {
                    Text("• Uploaded data is retained for 3 days")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)

                    Text("• Failed uploads are retried up to 5 times")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)

                    Text("• Storage warnings appear below 50MB")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)

                    Text("• Data collection pauses below 10MB")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                }
            }
            .scrollContentBackground(.hidden)
            .background(Color.warmBackground)
            .navigationTitle("Storage Details")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
            .onAppear {
                calculateStorageInfo()
            }
        }
    }
    
    private func calculateStorageInfo() {
        // Database size
        let dbSize = sqliteManager.getTotalDatabaseSize()
        databaseSize = formatBytes(dbSize)
        
        // Available storage
        if let systemAttributes = try? FileManager.default.attributesOfFileSystem(
            forPath: NSHomeDirectory()
        ) {
            if let freeSpace = systemAttributes[.systemFreeSize] as? Int64 {
                availableStorage = formatBytes(freeSpace)
            }
        }
    }
    
    private func formatBytes(_ bytes: Int64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .binary
        return formatter.string(fromByteCount: bytes)
    }
}

struct DetailRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
            Spacer()
            Text(value)
                .foregroundColor(.warmForegroundMuted)
        }
    }
}

// MARK: - Preview

struct SettingsView_Previews: PreviewProvider {
    static var previews: some View {
        SettingsView()
    }
}