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
    @State private var showLinkCodeEntry = false
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

                    // Enter a linking code (fully-remote — pair through a device
                    // you already have, no QR / no LAN needed).
                    Button(action: {
                        Haptics.light()
                        pairingError = nil
                        showLinkCodeEntry = true
                    }) {
                        Label("Enter Linking Code", systemImage: "keyboard")
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
            .sheet(isPresented: $showLinkCodeEntry) {
                ManualCodeEntryView(
                    onEnter: handleLinkCode,
                    onCancel: { showLinkCodeEntry = false }
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
    /// Fully-remote link: the user typed a code shown on an already-paired device.
    /// Resolve the box via atlas, wait for approval, pull the bearer over iroh.
    private func handleLinkCode(_ code: String) {
        showLinkCodeEntry = false
        isCompletingPairing = true
        pairingError = nil
        Task {
            do {
                try await NetworkManager.shared.linkDevice(code: code)
                await MainActor.run {
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

                // iroh model: keep the scanned origin as the path base
                // (`apiEndpoint`); actual reach is over iroh via the ticket
                // (`box_node_id` + `relay_url`) the box just returned, dialed by
                // BoxTransport. The device seed was generated + stored in
                // `consumePairToken`.
                await MainActor.run {
                    deviceManager.updateConfiguration(apiEndpoint: endpoint)
                    deviceManager.updateReach(boxNodeId: response.boxNodeId, relayUrl: response.relayUrl)
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

    /// Desktop-RELAYED provision (Mac→phone hand-off): an already-paired device
    /// asks the box to mint this phone's credential, then shows a v2 QR carrying
    /// the box's iroh reach ticket + bearer:
    ///
    /// ```json
    /// { "v": 2, "box_node_id": "...", "relay_url": "...",
    ///   "bearer": "...", "credential_id": "...", "device_id": "..." }
    /// ```
    ///
    /// We store the bearer + ticket, generate this device's iroh seed, and
    /// register its EndpointId with the box so it's allowlisted. (The box minted
    /// the credential before the phone had a key, so the node_id is registered
    /// here post-scan rather than in-band — see `registerSelfNodeId`.)
    private func handleBundleScanResult(_ payload: Data) {
        showQRScanner = false
        isCompletingPairing = true
        pairingError = nil

        Task {
            do {
                guard let root = try JSONSerialization.jsonObject(with: payload) as? [String: Any] else {
                    throw NetworkError.decodingError
                }
                let bearer = root["bearer"] as? String
                let boxNodeId = root["box_node_id"] as? String
                let relayUrl = root["relay_url"] as? String
                guard let bearer, !bearer.isEmpty,
                      let boxNodeId, !boxNodeId.isEmpty,
                      let relayUrl, !relayUrl.isEmpty else {
                    throw NetworkError.badRequest(
                        message: "This provision QR is missing the box reach ticket. "
                            + "Regenerate it from + Add Device on your other device."
                    )
                }

                // Persist bearer + reach ticket + this device's iroh seed.
                try KeychainStore.shared.saveBearer(bearer)
                let seed = NetworkManager.ensureIrohSeed()
                let nodeId = seed.flatMap { try? endpointIdFromSeed(deviceSeedHex: $0) }

                // `apiEndpoint` is only a path base over iroh (host is ignored by
                // the box) — provision QRs carry no LAN origin, so use a stable
                // placeholder so webhook paths compose.
                let pathBase = "http://virtues.box:8000"

                await MainActor.run {
                    deviceManager.updateConfiguration(apiEndpoint: pathBase)
                    deviceManager.updateReach(boxNodeId: boxNodeId, relayUrl: relayUrl)
                    deviceManager.isConfigured = true
                    deviceManager.configurationState = .configured
                    isCompletingPairing = false
                    Haptics.success()
                }

                // Register this device's EndpointId so the box allowlists it, and
                // bump `last_seen_at` so the relaying device flips to "paired".
                // Best-effort + detached: the first upload retries if it doesn't
                // land now.
                if let nodeId, let base = DeviceManager.shared.configuration.baseURL {
                    Task.detached {
                        _ = await NetworkManager.shared.registerSelfNodeId(
                            base: base, bearer: bearer, nodeId: nodeId
                        )
                    }
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

        // Drop the warm iroh connection + wipe all Keychain secrets.
        Task { await BoxTransport.shared.reset() }
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