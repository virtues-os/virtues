//
//  DataView.swift
//  Virtues
//
//  Logger-style view with toggles for each data stream
//

import SwiftUI
import AVFoundation
import CoreLocation

// MARK: - Permission State

// MARK: - Data Tab Selection

enum DataTab: String, CaseIterable {
    case streams = "Streams"
    case activity = "Activity"
}

enum PermissionState {
    case granted
    case denied
    case undetermined
    case notRequired  // Battery
    case partial      // Location "When In Use" instead of "Always"

    var icon: String {
        switch self {
        case .granted, .notRequired: return "checkmark.circle.fill"
        case .denied: return "xmark.circle.fill"
        case .undetermined: return "questionmark.circle.fill"
        case .partial: return "exclamationmark.circle.fill"
        }
    }

    var color: Color {
        switch self {
        case .granted, .notRequired: return .warmSuccess
        case .denied: return .warmError
        case .undetermined, .partial: return .warmWarning
        }
    }
}

struct DataView: View {
    @ObservedObject private var healthKitManager = HealthKitManager.shared
    @ObservedObject private var locationManager = LocationManager.shared
    @ObservedObject private var audioManager = AudioManager.shared
    @ObservedObject private var batteryManager = BatteryManager.shared
    @ObservedObject private var contactsManager = ContactsManager.shared
    @ObservedObject private var barometerManager = BarometerManager.shared
    @ObservedObject private var financeKitManager = FinanceKitManager.shared
    @ObservedObject private var eventKitManager = EventKitManager.shared

    // Tab selection
    @State private var selectedTab: DataTab = .streams

    // Info sheet state
    @State private var showHealthKitInfo = false
    @State private var showLocationInfo = false
    @State private var showAudioInfo = false
    @State private var showContactsInfo = false
    @State private var showBatteryInfo = false
    @State private var showBarometerInfo = false
    @State private var showFinanceKitInfo = false
    @State private var showEventKitInfo = false

    // Permission denial alert state
    @State private var showPermissionDeniedAlert = false
    @State private var deniedPermissionType: String = ""
    @State private var deniedPermissionMessage: String = ""

    var body: some View {
        NavigationView {
            VStack(spacing: 0) {
                // Segmented picker for tab selection
                Picker("", selection: $selectedTab) {
                    ForEach(DataTab.allCases, id: \.self) { tab in
                        Text(tab.rawValue).tag(tab)
                    }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal, 16)
                .padding(.vertical, 12)

                // Content based on selected tab
                switch selectedTab {
                case .streams:
                    streamsList
                case .activity:
                    ActivityLogView()
                }
            }
            .background(Color.warmBackground)
            .navigationTitle("Data")
            .navigationBarTitleDisplayMode(.inline)
        }
        .navigationViewStyle(StackNavigationViewStyle())
        .sheet(isPresented: $showHealthKitInfo) {
            HealthKitInfoView()
        }
        .sheet(isPresented: $showLocationInfo) {
            LocationInfoView()
        }
        .sheet(isPresented: $showAudioInfo) {
            AudioInfoView()
        }
        .sheet(isPresented: $showContactsInfo) {
            ContactsInfoView()
        }
        .sheet(isPresented: $showBatteryInfo) {
            BatteryInfoView()
        }
        .sheet(isPresented: $showBarometerInfo) {
            BarometerInfoView()
        }
        .sheet(isPresented: $showFinanceKitInfo) {
            FinanceKitInfoView()
        }
        .sheet(isPresented: $showEventKitInfo) {
            EventKitInfoView()
        }
        .alert("Permission Required", isPresented: $showPermissionDeniedAlert) {
            Button("OK", role: .cancel) { }
        } message: {
            Text(deniedPermissionMessage)
        }
    }

    // MARK: - Streams List

    private var streamsList: some View {
        List {
            // Warning banner when additional sensors are on but location (core sensor) is off
            if hasAdditionalSensorsWithoutLocation {
                NoCoreSensorBanner()
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
            }
            // Location upgrade banner when permission is "While Using" instead of "Always"
            else if locationPermissionState == .partial {
                LocationUpgradeBanner()
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
            }

            // MARK: - Core Sensor
            Section {
                StreamToggleRow(
                    icon: "location.fill",
                    iconColor: .warmInfo,
                    title: "Location",
                    subtitle: "GPS tracking",
                    isEnabled: locationEnabled,
                    onToggle: { enabled in
                        Task { await toggleLocation(enabled) }
                    },
                    onInfoTap: { showLocationInfo = true },
                    permissionState: locationPermissionState,
                    onPermissionTap: locationPermissionState == .denied ? openSettings : nil
                )
            } header: {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Core Sensor")
                        .font(.subheadline)
                        .fontWeight(.semibold)
                        .foregroundColor(.primary)
                    Text("Required for continuous background recording")
                        .font(.caption)
                        .foregroundColor(.warmForegroundMuted)
                        .fontWeight(.regular)
                }
                .textCase(nil)
            }

            // MARK: - Additional Sensors
            Section {
                StreamToggleRow(
                    icon: "mic.fill",
                    iconColor: .warmSuccess,
                    title: "Audio",
                    subtitle: audioEnabled ? "Recording" : "Voice recording",
                    isEnabled: audioEnabled,
                    onToggle: { enabled in
                        Task { await toggleAudio(enabled) }
                    },
                    onInfoTap: { showAudioInfo = true },
                    permissionState: audioPermissionState,
                    onPermissionTap: audioPermissionState == .denied ? openSettings : nil
                )

                StreamToggleRow(
                    icon: "heart.fill",
                    iconColor: .warmError,
                    title: "HealthKit",
                    subtitle: "Health & fitness",
                    isEnabled: healthKitEnabled,
                    onToggle: { enabled in
                        Task { await toggleHealthKit(enabled) }
                    },
                    onInfoTap: { showHealthKitInfo = true },
                    permissionState: healthKitPermissionState,
                    onPermissionTap: healthKitPermissionState == .denied ? openSettings : nil
                )

                StreamToggleRow(
                    icon: "person.crop.circle",
                    iconColor: .cyan,
                    title: "Contacts",
                    subtitle: "Address book",
                    isEnabled: contactsEnabled,
                    onToggle: { enabled in
                        Task { await toggleContacts(enabled) }
                    },
                    onInfoTap: { showContactsInfo = true },
                    permissionState: contactsPermissionState,
                    onPermissionTap: contactsPermissionState == .denied ? openSettings : nil
                )

                StreamToggleRow(
                    icon: "creditcard.fill",
                    iconColor: .green,
                    title: "FinanceKit",
                    subtitle: "Apple Card & Cash",
                    isEnabled: financeKitEnabled,
                    onToggle: { enabled in
                        Task { await toggleFinanceKit(enabled) }
                    },
                    onInfoTap: { showFinanceKitInfo = true },
                    permissionState: financeKitPermissionState,
                    onPermissionTap: financeKitPermissionState == .denied ? openSettings : nil
                )

                StreamToggleRow(
                    icon: "calendar",
                    iconColor: .red,
                    title: "EventKit",
                    subtitle: "Calendar & Reminders",
                    isEnabled: eventKitEnabled,
                    onToggle: { enabled in
                        Task { await toggleEventKit(enabled) }
                    },
                    onInfoTap: { showEventKitInfo = true },
                    permissionState: eventKitPermissionState,
                    onPermissionTap: eventKitPermissionState == .denied ? openSettings : nil
                )

                StreamToggleRow(
                    icon: "battery.75percent",
                    iconColor: .warmSuccess,
                    title: "Battery",
                    subtitle: "Device status",
                    isEnabled: batteryEnabled,
                    onToggle: { enabled in
                        toggleBattery(enabled)
                    },
                    onInfoTap: { showBatteryInfo = true },
                    permissionState: .notRequired
                )

                if BarometerManager.isAvailable {
                    StreamToggleRow(
                        icon: "barometer",
                        iconColor: .purple,
                        title: "Barometer",
                        subtitle: barometerEnabled ? "Pressure & altitude" : "Pressure & altitude",
                        isEnabled: barometerEnabled,
                        onToggle: { enabled in
                            toggleBarometer(enabled)
                        },
                        onInfoTap: { showBarometerInfo = true },
                        permissionState: .notRequired
                    )
                }
            } header: {
                Text("Additional Sensors")
                    .font(.subheadline)
                    .fontWeight(.semibold)
                    .foregroundColor(.primary)
                    .textCase(nil)
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
    }

    // MARK: - Stream State

    private var healthKitEnabled: Bool {
        healthKitManager.isMonitoring
    }

    private var locationEnabled: Bool {
        locationManager.isTracking
    }

    private var audioEnabled: Bool {
        audioManager.isRecording
    }

    private var contactsEnabled: Bool {
        contactsManager.isEnabled
    }

    private var batteryEnabled: Bool {
        batteryManager.isMonitoring
    }

    private var barometerEnabled: Bool {
        barometerManager.isMonitoring
    }

    private var financeKitEnabled: Bool {
        financeKitManager.isMonitoring
    }

    private var eventKitEnabled: Bool {
        eventKitManager.isMonitoring
    }

    // MARK: - Permission States

    private var healthKitPermissionState: PermissionState {
        if healthKitManager.isAuthorized {
            return .granted
        }
        return healthKitManager.hasRequestedHealthKitAuthorization ? .denied : .undetermined
    }

    private var locationPermissionState: PermissionState {
        switch locationManager.authorizationStatus {
        case .authorizedAlways:
            return .granted
        case .authorizedWhenInUse:
            return .partial
        case .denied, .restricted:
            return .denied
        case .notDetermined:
            return .undetermined
        @unknown default:
            return .undetermined
        }
    }

    private var audioPermissionState: PermissionState {
        switch audioManager.microphoneAuthorizationStatus {
        case .granted:
            return .granted
        case .denied:
            return .denied
        case .undetermined:
            return .undetermined
        @unknown default:
            return .undetermined
        }
    }

    private var contactsPermissionState: PermissionState {
        contactsManager.isAuthorized ? .granted : .undetermined
    }

    private var financeKitPermissionState: PermissionState {
        if financeKitManager.isAuthorized {
            return .granted
        }
        return financeKitManager.hasRequestedFinanceKitAuthorization ? .denied : .undetermined
    }

    private var eventKitPermissionState: PermissionState {
        if eventKitManager.hasAnyPermission {
            return .granted
        }
        // EventKit doesn't track "has requested" so use undetermined
        return .undetermined
    }

    /// True if any additional sensor is enabled but location (the core sensor) is not
    private var hasAdditionalSensorsWithoutLocation: Bool {
        let anyAdditionalEnabled = audioEnabled || healthKitEnabled || contactsEnabled || batteryEnabled || barometerEnabled || financeKitEnabled || eventKitEnabled
        return anyAdditionalEnabled && !locationEnabled
    }

    private func openSettings() {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
    }

    // MARK: - Toggle Actions

    private func toggleHealthKit(_ enabled: Bool) async {
        if enabled {
            // Request permission if not granted
            if !healthKitManager.isAuthorized {
                let granted = await healthKitManager.requestAuthorization()
                if granted {
                    healthKitManager.startMonitoring()
                } else {
                    showPermissionDenied(
                        type: "HealthKit",
                        message: "Health data collection requires HealthKit access. Please enable it in Settings to track your heart rate, steps, and workouts."
                    )
                }
            } else {
                healthKitManager.startMonitoring()
            }
        } else {
            healthKitManager.stopMonitoring()
        }
    }

    private func toggleLocation(_ enabled: Bool) async {
        if enabled {
            // Request "When In Use" permission if not granted
            if !locationManager.hasPermission {
                _ = await locationManager.requestAuthorization()
            }
            // After getting When In Use, request upgrade to Always for background tracking
            if locationManager.hasPermission && !locationManager.hasAlwaysPermission {
                _ = await locationManager.requestAlwaysAuthorization()
            }
            if locationManager.hasPermission {
                locationManager.startTracking()
            } else {
                showPermissionDenied(
                    type: "Location",
                    message: "Location tracking requires permission for data collection. Please enable it in Settings."
                )
            }
        } else {
            locationManager.stopTracking()
        }
    }

    private func toggleAudio(_ enabled: Bool) async {
        if enabled {
            // Request permission if not granted
            if !audioManager.hasPermission {
                _ = await audioManager.requestAuthorization()
            }
            if audioManager.hasPermission {
                audioManager.startRecording()
            } else {
                showPermissionDenied(
                    type: "Microphone",
                    message: "Audio recording requires microphone access to capture and transcribe your voice. Please enable it in Settings."
                )
            }
        } else {
            audioManager.stopRecording()
        }
    }

    private func toggleContacts(_ enabled: Bool) async {
        if enabled {
            if !contactsManager.isAuthorized {
                let granted = await contactsManager.requestAuthorization()
                if granted {
                    contactsManager.startSyncing()
                } else {
                    showPermissionDenied(
                        type: "Contacts",
                        message: "Contact access helps identify people mentioned in your conversations. Please enable it in Settings."
                    )
                }
            } else {
                contactsManager.startSyncing()
            }
        } else {
            contactsManager.stopSyncing()
        }
    }

    private func toggleFinanceKit(_ enabled: Bool) async {
        if enabled {
            if !financeKitManager.isAuthorized {
                let granted = await financeKitManager.requestAuthorization()
                if granted {
                    financeKitManager.startMonitoring()
                } else {
                    showPermissionDenied(
                        type: "FinanceKit",
                        message: "Financial data collection requires FinanceKit access. Please enable it in Settings to track Apple Card and Apple Cash transactions."
                    )
                }
            } else {
                financeKitManager.startMonitoring()
            }
        } else {
            financeKitManager.stopMonitoring()
        }
    }

    private func toggleEventKit(_ enabled: Bool) async {
        if enabled {
            if !eventKitManager.hasAnyPermission {
                let calendarGranted = await eventKitManager.requestCalendarAuthorization()
                let remindersGranted = await eventKitManager.requestRemindersAuthorization()
                if calendarGranted || remindersGranted {
                    eventKitManager.startMonitoring()
                } else {
                    showPermissionDenied(
                        type: "EventKit",
                        message: "Calendar and Reminders access is needed to sync your events. Please enable it in Settings."
                    )
                }
            } else {
                eventKitManager.startMonitoring()
            }
        } else {
            eventKitManager.stopMonitoring()
        }
    }

    private func showPermissionDenied(type: String, message: String) {
        deniedPermissionType = type
        deniedPermissionMessage = message
        showPermissionDeniedAlert = true
    }

    private func toggleBattery(_ enabled: Bool) {
        if enabled {
            batteryManager.startMonitoring()
        } else {
            batteryManager.stopMonitoring()
        }
    }

    private func toggleBarometer(_ enabled: Bool) {
        if enabled {
            barometerManager.startMonitoring()
        } else {
            barometerManager.stopMonitoring()
        }
    }
}

// MARK: - No Core Sensor Banner

/// Banner shown when additional sensors are enabled but location (core sensor) is not
struct NoCoreSensorBanner: View {
    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.title2)
                .foregroundColor(.warmWarning)
                .frame(width: 32)

            VStack(alignment: .leading, spacing: 2) {
                Text("Background Recording Disabled")
                    .font(.subheadline)
                    .fontWeight(.semibold)

                Text("Enable Location to record when app is closed")
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
            }

            Spacer()
        }
        .padding(12)
        .background(Color.warmWarning.opacity(0.1))
        .cornerRadius(12)
        .padding(.horizontal, 4)
        .padding(.vertical, 8)
    }
}

// MARK: - Location Upgrade Banner

/// Banner shown when location permission is "While Using" instead of "Always"
struct LocationUpgradeBanner: View {
    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "location.fill")
                .font(.title2)
                .foregroundColor(.warmWarning)
                .frame(width: 32)

            VStack(alignment: .leading, spacing: 2) {
                Text("Limited Background Recording")
                    .font(.subheadline)
                    .fontWeight(.semibold)

                Text("Enable 'Always' location in Settings for continuous logging")
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
            }

            Spacer()
        }
        .padding(12)
        .background(Color.warmWarning.opacity(0.1))
        .cornerRadius(12)
        .padding(.horizontal, 4)
        .padding(.vertical, 8)
    }
}

// MARK: - Stream Toggle Row

struct StreamToggleRow: View {
    let icon: String
    let iconColor: Color
    let title: String
    let subtitle: String
    let isEnabled: Bool
    let onToggle: (Bool) -> Void
    var onInfoTap: (() -> Void)? = nil
    var permissionState: PermissionState? = nil
    var onPermissionTap: (() -> Void)? = nil

    var body: some View {
        HStack(spacing: 12) {
            // Icon
            Image(systemName: icon)
                .font(.title2)
                .foregroundColor(iconColor)
                .frame(width: 32)

            // Title and subtitle
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.body)
                    .fontWeight(.medium)

                Text(subtitle)
                    .font(.caption)
                    .foregroundColor(.warmForegroundMuted)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            // Info button + Toggle grouped together
            HStack(spacing: 8) {
                if let onInfoTap = onInfoTap {
                    Button(action: {
                        Haptics.light()
                        onInfoTap()
                    }) {
                        Image(systemName: "info.circle")
                            .font(.body)
                            .foregroundColor(.warmForegroundMuted)
                    }
                    .buttonStyle(PlainButtonStyle())
                }

                Toggle("", isOn: Binding(
                    get: { isEnabled },
                    set: { newValue in
                        Haptics.light()
                        onToggle(newValue)
                    }
                ))
                .toggleStyle(.warm)
                .labelsHidden()
            }
        }
        .padding(.vertical, 4)
    }
}

#Preview {
    DataView()
}
