//
//  BarometerInfoView.swift
//  Virtues
//
//  Info and configuration page for Barometer data stream
//

import SwiftUI

struct BarometerInfoView: View {
    @ObservedObject private var barometerManager = BarometerManager.shared
    @ObservedObject private var uploadCoordinator = BatchUploadCoordinator.shared
    @Environment(\.dismiss) private var dismiss

    // Sample data for preview
    private let sampleJSON = """
    {
      "device_id": "ABC12345",
      "metrics": [
        {
          "timestamp": "2025-01-15T10:30:00Z",
          "pressure_kpa": 101.325,
          "relative_altitude_meters": 0.0
        },
        {
          "timestamp": "2025-01-15T10:35:00Z",
          "pressure_kpa": 101.290,
          "relative_altitude_meters": 2.5
        }
      ]
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

                    // Data Preview
                    dataPreviewSection

                    // Privacy
                    privacySection
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 20)
            }
            .background(Color.warmBackground)
            .navigationTitle("Barometer")
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
            Image(systemName: "barometer")
                .font(.system(size: 44))
                .foregroundColor(.purple)

            VStack(alignment: .leading, spacing: 4) {
                Text("Barometric Pressure")
                    .font(.title2)
                    .fontWeight(.bold)

                Text("Pressure & relative altitude")
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

            if barometerManager.isMonitoring {
                if let pressure = barometerManager.currentPressure {
                    InfoRow(label: "Current Pressure", value: String(format: "%.2f kPa", pressure))
                }

                if let altitude = barometerManager.relativeAltitude {
                    InfoRow(label: "Relative Altitude", value: String(format: "%.1f m", altitude))
                }
            }

            InfoRow(label: "Permission", value: "Not Required", valueColor: .warmSuccess)
        }
        .padding(16)
        .background(Color.warmSurfaceElevated)
        .cornerRadius(12)
    }

    private var currentStatus: StreamStatus {
        if !BarometerManager.isAvailable {
            return .error("Not available")
        } else if barometerManager.isMonitoring {
            return .active
        } else {
            return .disabled
        }
    }

    private var aboutSection: some View {
        InfoSection(title: "About", icon: "info.circle") {
            Text("This stream captures barometric pressure and relative altitude changes from your device's built-in barometer sensor.")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)

            Text("What's captured:")
                .font(.subheadline)
                .fontWeight(.medium)
                .padding(.top, 4)

            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("Atmospheric pressure (kPa)")
                bulletPoint("Relative altitude changes (meters)")
                bulletPoint("Timestamps for all readings")
            }
            .padding(.vertical, 4)

            Text("This data is useful for detecting elevation changes, weather patterns, and activity context (stairs vs elevator).")
                .font(.subheadline)
                .foregroundColor(.warmForegroundMuted)
        }
    }

    private var dataPreviewSection: some View {
        InfoSection(title: "Data Preview", icon: "doc.text") {
            DataPreviewCard(title: "Sample Barometer Data", jsonString: sampleJSON)
        }
    }

    private var privacySection: some View {
        InfoSection(title: "Privacy & Battery", icon: "hand.raised") {
            VStack(alignment: .leading, spacing: 8) {
                bulletPoint("No permission required")
                bulletPoint("Uses on-device sensor only")
                bulletPoint("Batched uploads every 5 minutes")
                bulletPoint("No location data exposed")
            }
            .foregroundColor(.warmForegroundMuted)
            .padding(.vertical, 4)

            Divider()

            InfoRow(label: "Battery Impact", value: "Very Low", valueColor: .warmSuccess)

            Text("The barometer sensor is extremely power-efficient.")
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
}

#Preview {
    BarometerInfoView()
}
