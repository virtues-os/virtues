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
    @State private var isForceSyncing = false
    @State private var forceSyncResult: String?
    
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

                    // Connection / WireGuard tunnel details (status, endpoint,
                    // SPKI fingerprint, forget credentials).
                    NavigationLink(destination: ConnectionSettingsView()) {
                        Label("Connection", systemImage: "lock.shield")
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

                    // Last *successful* sync is the signal that matters — data
                    // actually reached the box. Shown in green so it reads as the
                    // health indicator.
                    if let lastSuccess = uploadCoordinator.lastSuccessfulSyncDate {
                        HStack {
                            Text("Last Successful Sync")
                            Spacer()
                            Text(lastSuccess, style: .relative)
                                .foregroundColor(.warmSuccess)
                        }
                    } else {
                        HStack {
                            Text("Last Successful Sync")
                            Spacer()
                            Text("Never")
                                .foregroundColor(.warmForegroundMuted)
                        }
                    }

                    // Last *attempt* is a separate, weaker signal — a recent
                    // attempt can still have failed, so it is never shown as the
                    // success line (that conflation is what hid broken syncs).
                    if let lastAttempt = uploadCoordinator.lastUploadDate,
                       lastAttempt != uploadCoordinator.lastSuccessfulSyncDate {
                        HStack {
                            Text("Last Attempt")
                            Spacer()
                            Text(lastAttempt, style: .relative)
                                .foregroundColor(.warmForegroundMuted)
                        }
                    }

                    // Manual upload trigger — clears stuck error state and
                    // forces an immediate upload of all pending events.
                    Button(action: {
                        Haptics.light()
                        isForceSyncing = true
                        forceSyncResult = nil
                        Task {
                            let ok = await uploadCoordinator.forceUpload()
                            await MainActor.run {
                                isForceSyncing = false
                                forceSyncResult = ok ? "✓ Upload sent" : "✗ Upload failed (see logs)"
                                Haptics.success()
                                DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                                    forceSyncResult = nil
                                }
                            }
                        }
                    }) {
                        HStack {
                            if isForceSyncing {
                                ProgressView()
                                    .progressViewStyle(CircularProgressViewStyle())
                                    .scaleEffect(0.8)
                                Text("Sending...")
                            } else {
                                Label("Send Now", systemImage: "paperplane.fill")
                            }
                        }
                        .foregroundColor(.warmPrimary)
                    }
                    .disabled(isForceSyncing)

                    if let result = forceSyncResult {
                        Text(result)
                            .font(.caption)
                            .foregroundColor(.warmForegroundMuted)
                    }
                }

                // Device Identity Section
                Section(header: Text("Device Identity")) {
                    HStack(alignment: .top) {
                        Text("Device ID")
                        Spacer()
                        Text(deviceManager.deviceId)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundColor(.warmForegroundMuted)
                            .multilineTextAlignment(.trailing)
                            .textSelection(.enabled)
                    }

                    Button(action: {
                        Haptics.light()
                        UIPasteboard.general.string = deviceManager.deviceId
                        withAnimation { showCopiedToast = true }
                        DispatchQueue.main.asyncAfter(deadline: .now() + 1.8) {
                            withAnimation { showCopiedToast = false }
                        }
                    }) {
                        Label("Copy Device ID", systemImage: "doc.on.doc")
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
                        Label("Unpair & Reset", systemImage: "exclamationmark.triangle")
                            .foregroundColor(.warmError)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Color.warmBackground)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .alert("Unpair & Reset?", isPresented: $showingResetAlert) {
                Button("Cancel", role: .cancel) { }
                Button("Unpair & Reset", role: .destructive) {
                    resetApp()
                }
            } message: {
                Text("Fully disconnects this phone from your box: wipes its credentials, tunnel, and all settings. You'll set it up again from scratch. Pending uploads will be lost.")
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
                    onBundleScanned: handleBundleScanResult,
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
    
    /// QR carries a `/pair#t=<token>` URL produced by `virtues link` on the
    /// box or the "+ Add device" modal in `/virtues/devices`. The scanner
    /// extracts `(endpoint, token)`; we POST to `/api/pair/consume` with
    /// `kind = "mobile_app"`, persist the server-issued bearer into the
    /// Keychain (done inside `consumePairToken`), and persist the endpoint
    /// + action_ids into `DeviceConfiguration`.
    private func handleQRScanResult(endpoint: String, pairToken: String, fingerprint: String?) {
        showQRScanner = false
        isCompletingPairing = true
        pairingError = nil

        Task {
            do {
                let response = try await NetworkManager.shared.consumePairToken(
                    endpoint: endpoint,
                    pairToken: pairToken,
                    deviceId: DeviceManager.shared.deviceId,
                    expectedFingerprint: fingerprint
                )

                // Reach the box from now on at its relay URL (works off-LAN);
                // fall back to the scanned origin on a LAN-only box.
                let reachEndpoint = response.boxUrl?.isEmpty == false
                    ? response.boxUrl!
                    : endpoint
                await MainActor.run {
                    deviceManager.updateConfiguration(apiEndpoint: reachEndpoint)
                    deviceManager.updateActionIds(response.actionIds)
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

    /// Desktop-RELAYED off-LAN pairing: the QR is a `virtues-bundle:` blob whose
    /// envelope is `{ bundle, action_ids }`. The box already generated this
    /// device's WG keypair, so there's no consume round-trip — we import the
    /// bundle (bearer + private key + WG params), pin the box's key (TOFU; no
    /// out-of-band fpr needed since the QR came from the user's own already-paired
    /// device), set the tunnel endpoint, persist `action_ids`, and ping the box
    /// over the fresh tunnel so the relaying device's UI flips to "paired".
    private func handleBundleScanResult(_ envelope: Data) {
        showQRScanner = false
        isCompletingPairing = true
        pairingError = nil

        Task {
            do {
                guard
                    let root = try JSONSerialization.jsonObject(with: envelope) as? [String: Any],
                    let bundle = root["bundle"] as? [String: Any],
                    let bundleData = try? JSONSerialization.data(withJSONObject: bundle)
                else {
                    throw NetworkError.decodingError
                }

                let bearer = bundle["bearer"] as? String
                let wg = bundle["wg"] as? [String: Any]
                let privKey = wg?["client_private_key"] as? String
                guard let bearer, !bearer.isEmpty, let privKey, !privKey.isEmpty else {
                    throw NetworkError.badRequest(
                        message: "This pairing QR is missing tunnel credentials. "
                            + "Regenerate it from + Add Device on your other device."
                    )
                }

                // Store secrets + bundle. `verifyAndStoreBundle` pins the server
                // key (TOFU) and tears down any stale tunnel; nil fingerprint =
                // trust the bundle from the user's own paired device.
                try KeychainStore.shared.saveBearer(bearer)
                try KeychainStore.shared.saveWgPrivateKey(privKey)
                try VirtuesTunnelManager.shared.verifyAndStoreBundle(
                    bundleData,
                    expectedFingerprint: nil
                )

                let actionIds = (root["action_ids"] as? [String: String]) ?? [:]
                let internalHost = (bundle["internal_host"] as? String) ?? "virtues.internal"
                let httpPort = (bundle["http_port"] as? Int) ?? 8000
                let endpoint = "http://\(internalHost):\(httpPort)"

                await MainActor.run {
                    deviceManager.updateConfiguration(apiEndpoint: endpoint)
                    deviceManager.updateActionIds(actionIds)
                    deviceManager.isConfigured = true
                    deviceManager.configurationState = .configured
                    isCompletingPairing = false
                    Haptics.success()
                }

                // Best-effort, non-blocking: reach the box over the new tunnel so
                // its `last_seen_at` advances and the relaying device shows
                // "paired" now (not on the first scheduled upload minutes later).
                // Detached so a slow/unreachable tunnel never stalls the UI — the
                // first upload registers liveness regardless.
                Task.detached { await NetworkManager.shared.confirmPairOnline() }
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
    
    /// Complete unpair: stop collection, tear down the tunnel, and wipe BOTH
    /// stores — Keychain (bearer, WG bundle, WG private key, server pin) AND
    /// UserDefaults (endpoint, action IDs, all settings). Previously this cleared
    /// only UserDefaults, so a dead endpoint / stale tunnel credentials survived
    /// a "reset" and the app kept dialing a ghost box. One action clears it all.
    private func resetApp() {
        // Stop all data collection.
        uploadCoordinator.stopPeriodicUploads()
        locationManager.stopTracking()
        audioManager.stopRecording()

        // Drop the live tunnel + wipe all Keychain secrets.
        VirtuesTunnelManager.shared.teardown()
        KeychainStore.shared.wipeAll()

        // Clear configuration (endpoint, action IDs) + every UserDefaults key.
        deviceManager.clearConfiguration()
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