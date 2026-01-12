//
//  LocationInfoView.swift
//  Virtues
//
//  Info and configuration page for Location data stream
//

import SwiftUI
import CoreLocation

struct LocationInfoView: View {
    @ObservedObject private var locationManager = LocationManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "device_id": "ABC12345",
      "data": {
        "latitude": 37.7749,
        "longitude": -122.4194,
        "altitude": 10.5,
        "accuracy": 5.0,
        "timestamp": "2025-01-15T10:30:00Z"
      }
    }
    """

    var body: some View {
        NavigationView {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Header
                    headerSection
                        .padding(.bottom, 8)

                    // Status
                    statusSection

                    // About
                    aboutSection

                    // Configuration
                    configSection

                    // Data Preview
                    dataPreviewSection

                    // Privacy & Battery
                    privacySection

                    // Troubleshooting
                    troubleshootingSection
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 20)
            }
            .background(Color.warmBackground)
            .navigationTitle("Location")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    // MARK: - Sections

    private var headerSection: some View {
        HStack(spacing: 16) {
            Image(systemName: "location.fill")
                .font(.system(size: 44))
                .foregroundColor(.warmInfo)

            VStack(alignment: .leading, spacing: 4) {
                Text("Location")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("GPS coordinates and movement tracking")
                    .font(.subheadline)
                    .foregroundColor(.warmForegroundMuted)
            }

            Spacer()
        }
        .padding(.horizontal, 4)
    }

    private var statusSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Status")
                    .font(.headline)
                Spacer()
                StreamStatusBadge(status: currentStatus)
            }

            Divider()

            InfoRow(label: "Pending Records", value: "\(uploadCoordinator.streamCounts.location)")

            if locationManager.isTracking {
                InfoRow(label: "Tracking Mode", value: "Continuous", valueColor: .warmInfo)
            }
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var currentStatus: StreamStatus {
        if !locationManager.hasPermission {
            return .error("Permission denied")
        } else if locationManager.isTracking {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream stores your location data to help build your personal timeline and understand your movement patterns.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            Text("Data stored:")
                .font(.subheadline)
                .fontWeight(.medium)
                .padding(.top, 4)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("GPS coordinates (latitude/longitude)")
                bulletPoint("Altitude")
                bulletPoint("Accuracy measurements")
                bulletPoint("Speed and direction (when moving)")
                bulletPoint("Timestamps")
            }
            .padding(.vertical, 4)

            Text("Location is sampled every 10 seconds when tracking is active.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var configSection: some View {
        InfoSection(title: "Configuration", icon: "slider.horizontal.3") {
            VStack(spacing: 10) {
                InfoRow(label: "Sampling Interval", value: "10 seconds")
                InfoRow(label: "Freshness Threshold", value: "30 seconds")
                InfoRow(label: "Accuracy", value: "10 meters")
                InfoRow(label: "Background Mode", value: "Always On")
            }

            Text("Configuration is managed by the server.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
                .padding(.top, 4)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample Location Record", jsonString: sampleJSON)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            Text("Data NOT stored:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Place names or addresses")
                bulletPoint("Visited business information")
                bulletPoint("Contact location data")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Moderate", valueColor: .warmWarning)

            Text("Continuous location tracking uses GPS and impacts battery life. Consider disabling when not needed.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)

            Text("Tip: The app uses efficient location APIs and reduces accuracy when stationary to minimize battery usage.")
                .font(.caption)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var troubleshootingSection: some View {
        InfoSection(title: "Troubleshooting", icon: "wrench") {
            SimplePermissionRow(title: "Location Access", isGranted: locationManager.hasPermission)
            SimplePermissionRow(title: "Always Permission", isGranted: locationManager.hasAlwaysPermission)

            if !locationManager.hasPermission || !locationManager.hasAlwaysPermission {
                Button(action: {
                    Haptics.light()
                    openSettings()
                }) {
                    Label("Open Location Settings", systemImage: "location.circle")
                }
                .buttonStyle(.bordered)
                .tint(.warmPrimary)
            }

            Divider()

            Button(action: {
                Haptics.medium()
                if locationManager.isTracking {
                    locationManager.stopTracking()
                } else {
                    locationManager.startTracking()
                }
            }) {
                Label(
                    locationManager.isTracking ? "Stop Tracking" : "Start Tracking",
                    systemImage: locationManager.isTracking ? "stop.circle" : "play.circle"
                )
            }
            .buttonStyle(.bordered)
            .tint(locationManager.isTracking ? .warmError : .warmPrimary)
            .disabled(!locationManager.hasPermission)

            Divider()

            Text("Common issues:")
                .font(.subheadline)
                .fontWeight(.medium)

            VStack(alignment: .leading, spacing: 6) {
                Text("• No location: Check that Location Services are enabled")
                Text("• Background tracking: Requires 'Always' permission")
                Text("• Inaccurate: Move to an open area for better GPS signal")
            }
            .font(.caption)
            .foregroundColor(.warmForegroundMuted)
        }
    }

    // MARK: - Helpers

    private func bulletPoint(_ text: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text("•")
            Text(text)
        }
        .font(.subheadline)
    }

    private func openSettings() {
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
    }
}

#Preview {
    LocationInfoView()
}
